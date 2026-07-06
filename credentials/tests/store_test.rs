//! CredentialStore 集成测试 — CRUD + 加密验证
//!
//! 使用 tempfile 创建临时 SQLite 数据库，每次测试独立隔离。

use lingshu_credentials::*;

/// 创建临时 CredentialStore
fn setup_store() -> (CredentialStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let db_path = dir.path().join("credentials.db");
    let store = CredentialStore::open(&db_path, "test-master-key-00000000000000000000000000")
        .expect("open credential store");
    (store, dir)
}

/// 创建测试凭证条目
fn make_entry(id: &str) -> CredentialEntry {
    let now = chrono::Utc::now().timestamp();
    CredentialEntry {
        id: id.to_string(),
        provider: GitProvider::Gitee,
        credential_type: CredentialType::PersonalAccessToken,
        name: format!("test-{id}"),
        description: "test credential".into(),
        token: format!("ghp_test_token_{id}"),
        username: Some("testuser".into()),
        base_url: Some("https://gitee.com".into()),
        scopes: vec!["api".into(), "repo".into()],
        permissions_group: Some("dev".into()),
        expires_at: Some(now + 86400 * 365),
        created_at: now,
        updated_at: now,
    }
}

// ── 基础 CRUD ───────────────────────────────────────

#[test]
fn test_open_creates_db() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let db_path = dir.path().join("test.db");
    assert!(!db_path.exists(), "db should not exist yet");

    let store = CredentialStore::open(&db_path, "test-key").expect("open should create db");
    drop(store);

    assert!(db_path.exists(), "db should be created");
}

#[test]
fn test_insert_and_get() {
    let (store, _dir) = setup_store();
    let entry = make_entry("test-001");

    store.insert(&entry).expect("insert should succeed");

    let retrieved = store
        .get("test-001")
        .expect("get should succeed")
        .expect("entry should exist");

    assert_eq!(retrieved.id, "test-001");
    assert_eq!(retrieved.name, "test-test-001");
    assert_eq!(retrieved.token, "ghp_test_token_test-001"); // decrypted!
    assert_eq!(retrieved.provider, GitProvider::Gitee);
    assert_eq!(
        retrieved.credential_type,
        CredentialType::PersonalAccessToken
    );
    assert_eq!(retrieved.username, Some("testuser".into()));
    assert_eq!(retrieved.scopes, vec!["api", "repo"]);
    assert!(retrieved.expires_at.is_some());
}

#[test]
fn test_get_not_found() {
    let (store, _dir) = setup_store();
    let result = store
        .get("nonexistent")
        .expect("get nonexistent should succeed");
    assert!(result.is_none(), "nonexistent entry should be None");
}

#[test]
fn test_list_empty() {
    let (store, _dir) = setup_store();
    let list = store.list().expect("list should succeed");
    assert!(list.is_empty(), "new store should have empty list");
}

#[test]
fn test_list_after_insert() {
    let (store, _dir) = setup_store();
    store.insert(&make_entry("a")).expect("insert a");
    store.insert(&make_entry("b")).expect("insert b");

    let list = store.list().expect("list should succeed");
    assert_eq!(list.len(), 2, "should have 2 entries");

    // 验证摘要不暴露 token
    for summary in &list {
        assert!(
            !summary.masked_token.is_empty(),
            "masked_token should not be empty"
        );
        assert!(
            summary.masked_token.contains("..."),
            "masked_token should be masked: {}",
            summary.masked_token
        );
    }
}

#[test]
fn test_list_by_provider() {
    let (store, _dir) = setup_store();

    let mut gitee_entry = make_entry("g-001");
    gitee_entry.provider = GitProvider::Gitee;
    let mut coding_entry = make_entry("c-001");
    coding_entry.provider = GitProvider::Coding;

    store.insert(&gitee_entry).expect("insert gitee");
    store.insert(&coding_entry).expect("insert coding");

    let gitee_list = store.list_by_provider("gitee").expect("list gitee");
    assert_eq!(gitee_list.len(), 1);
    assert_eq!(gitee_list[0].provider, "gitee");

    let coding_list = store.list_by_provider("coding").expect("list coding");
    assert_eq!(coding_list.len(), 1);
    assert_eq!(coding_list[0].provider, "coding");

    let none_list = store.list_by_provider("gitcode").expect("list gitcode");
    assert!(none_list.is_empty());
}

#[test]
fn test_update() {
    let (store, _dir) = setup_store();
    store.insert(&make_entry("upd-001")).expect("insert");

    // 更新名称和 token
    let req = UpdateCredentialRequest {
        name: Some("updated-name".into()),
        description: None,
        token: Some("new-token-value".into()),
        username: None,
        base_url: None,
        scopes: Some(vec!["admin".into()]),
        permissions_group: None,
        expires_at: Some(9999999999),
    };

    let found = store
        .update("upd-001", &req)
        .expect("update should succeed");
    assert!(found, "update should return true for existing entry");

    let entry = store
        .get("upd-001")
        .expect("get should succeed")
        .expect("entry should exist after update");

    assert_eq!(entry.name, "updated-name");
    assert_eq!(entry.token, "new-token-value");
    assert_eq!(entry.scopes, vec!["admin"]);
    assert_eq!(entry.expires_at, Some(9999999999));
    // 未更新的字段应保持不变
    assert_eq!(entry.description, "test credential");
    assert_eq!(entry.username, Some("testuser".into()));
}

#[test]
fn test_update_not_found() {
    let (store, _dir) = setup_store();
    let req = UpdateCredentialRequest {
        name: Some("nope".into()),
        description: None,
        token: None,
        username: None,
        base_url: None,
        scopes: None,
        permissions_group: None,
        expires_at: None,
    };
    let found = store
        .update("nonexistent", &req)
        .expect("update nonexistent should succeed");
    assert!(!found, "update nonexistent should return false");
}

#[test]
fn test_delete() {
    let (store, _dir) = setup_store();
    store.insert(&make_entry("del-001")).expect("insert");

    let deleted = store.delete("del-001").expect("delete should succeed");
    assert!(deleted, "delete should return true");

    let entry = store.get("del-001").expect("get after delete");
    assert!(entry.is_none(), "entry should be gone after delete");
}

#[test]
fn test_delete_not_found() {
    let (store, _dir) = setup_store();
    let deleted = store
        .delete("nonexistent")
        .expect("delete nonexistent should succeed");
    assert!(!deleted, "delete nonexistent should return false");
}

// ── 加密验证 ────────────────────────────────────────

#[test]
fn test_encrypted_token_is_not_plaintext() {
    let (store, _dir) = setup_store();
    let entry = make_entry("enc-001");
    let plain_token = entry.token.clone();

    store.insert(&entry).expect("insert");

    // 直接从 SQLite 读原始数据验证已加密
    let store2 = CredentialStore::open(
        &_dir.path().join("credentials.db"),
        "different-key-wont-work-0000000000000000",
    )
    .expect("open with different key");
    drop(store2); // we only need this to not conflict

    let conn =
        rusqlite::Connection::open(_dir.path().join("credentials.db")).expect("open db directly");
    let enc_token: String = conn
        .query_row(
            "SELECT encrypted_token FROM credentials WHERE id = ?1",
            rusqlite::params!["enc-001"],
            |row| row.get::<_, String>(0),
        )
        .expect("query encrypted_token");

    assert_ne!(enc_token, plain_token, "token should be encrypted");
    assert!(!enc_token.is_empty(), "encrypted token should not be empty");
}

#[test]
fn test_decrypt_with_wrong_key_fails() {
    let (store, _dir) = setup_store();
    let entry = make_entry("key-001");
    store.insert(&entry).expect("insert");
    drop(store); // close first store

    // 用不同的 key 打开
    let store2 = CredentialStore::open(
        &_dir.path().join("credentials.db"),
        "different-master-key-00000000000000000000000",
    )
    .expect("open with different key");

    let result = store2.get("key-001");
    match result {
        Ok(_) => panic!("should fail to decrypt with wrong key"),
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("decrypt"),
                "error should mention decrypt: {msg}"
            );
        }
    }
}

// ── 多提供商 ────────────────────────────────────────

#[test]
fn test_all_provider_types() {
    let (store, _dir) = setup_store();

    let providers = [
        (GitProvider::Gitee, "personal_access_token"),
        (GitProvider::Codeup, "enterprise_token"),
        (GitProvider::Coding, "deployment_token"),
        (GitProvider::GitCode, "access_token"),
        (GitProvider::Cnb, "personal_access_token"),
    ];

    for (i, (provider, ct)) in providers.iter().enumerate() {
        let mut entry = make_entry(&format!("prov-{i:03}"));
        entry.provider = provider.clone();
        entry.credential_type = CredentialType::from_str(ct).unwrap();
        entry.token = format!("token-for-{:?}", provider);
        store.insert(&entry).expect("insert");
    }

    let list = store.list().expect("list");
    assert_eq!(list.len(), 5);

    // 验证按提供商过滤
    for (provider, _) in &providers {
        let filtered = store.list_by_provider(provider.as_str()).expect("filter");
        assert_eq!(
            filtered.len(),
            1,
            "should find 1 entry for {}",
            provider.as_str()
        );
        assert_eq!(filtered[0].provider, provider.as_str());
    }
}

// ── 边界条件 ────────────────────────────────────────

#[test]
fn test_empty_token() {
    let (store, _dir) = setup_store();
    let mut entry = make_entry("empty-token");
    entry.token = String::new();
    store.insert(&entry).expect("insert empty token");

    let retrieved = store
        .get("empty-token")
        .expect("get")
        .expect("entry should exist");
    assert_eq!(retrieved.token, "", "empty token roundtrip");
}

#[test]
fn test_very_long_token() {
    let (store, _dir) = setup_store();
    let mut entry = make_entry("long-token");
    entry.token = "A".repeat(10000);
    store.insert(&entry).expect("insert long token");

    let retrieved = store
        .get("long-token")
        .expect("get")
        .expect("entry should exist");
    assert_eq!(retrieved.token.len(), 10000);
    assert_eq!(retrieved.token, "A".repeat(10000));
}

#[test]
fn test_special_chars_in_token() {
    let (store, _dir) = setup_store();
    let mut entry = make_entry("special");
    entry.token = "!@#$%^&*()_+-={}[]|:;'<>,.?/~`你好".into();
    store.insert(&entry).expect("insert special chars");

    let retrieved = store
        .get("special")
        .expect("get")
        .expect("entry should exist");
    assert_eq!(retrieved.token, "!@#$%^&*()_+-={}[]|:;'<>,.?/~`你好");
}

#[test]
fn test_multiple_entries() {
    let (store, _dir) = setup_store();
    let n = 100;

    for i in 0..n {
        store
            .insert(&make_entry(&format!("multi-{i:03}")))
            .expect("insert");
    }

    let list = store.list().expect("list");
    assert_eq!(list.len(), n);

    // 随机抽查
    for i in (0..n).step_by(10) {
        let id = format!("multi-{i:03}");
        let entry = store
            .get(&id)
            .expect("get")
            .unwrap_or_else(|| panic!("entry {id} should exist"));
        assert_eq!(entry.token, format!("ghp_test_token_{id}"));
    }
}

#[test]
fn test_duplicate_insert_fails() {
    let (store, _dir) = setup_store();
    store.insert(&make_entry("dup")).expect("first insert");

    let dup = make_entry("dup");
    let result = store.insert(&dup);
    assert!(
        result.is_err(),
        "duplicate insert should fail due to PK constraint"
    );
}

#[test]
fn test_list_with_expired_and_active() {
    let (store, _dir) = setup_store();
    let now = chrono::Utc::now().timestamp();

    let mut active = make_entry("active");
    active.expires_at = Some(now + 86400 * 30); // 30 days from now
    store.insert(&active).expect("insert active");

    let mut expired = make_entry("expired");
    expired.expires_at = Some(now - 86400); // 1 day ago
    store.insert(&expired).expect("insert expired");

    let list = store.list().expect("list");
    assert_eq!(list.len(), 2);

    // 验证 expires_at 正确存储
    let active_summary = store.get("active").expect("get active").expect("exists");
    assert!(active_summary.expires_at.unwrap() > now);

    let expired_entry = store.get("expired").expect("get expired").expect("exists");
    assert!(expired_entry.expires_at.unwrap() < now);
}
