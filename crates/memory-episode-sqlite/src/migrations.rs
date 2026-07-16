//! SQLite 迁移脚本.

/// 创建 Episode 相关表的 SQL。
pub const MIGRATIONS: &[(&str, &str)] = &[
    ("001_episodes", r#"
CREATE TABLE IF NOT EXISTS episodes (
    id          TEXT PRIMARY KEY NOT NULL,
    title       TEXT NOT NULL,
    summary     TEXT NOT NULL DEFAULT '',
    timestamp   TEXT NOT NULL,
    session_id  TEXT,
    source_ref  TEXT,
    importance  REAL NOT NULL DEFAULT 0.5,
    metadata    TEXT NOT NULL DEFAULT '{}',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS episode_entities (
    episode_id  TEXT NOT NULL REFERENCES episodes(id) ON DELETE CASCADE,
    entity_kind TEXT NOT NULL,
    entity_name TEXT NOT NULL,
    PRIMARY KEY (episode_id, entity_kind, entity_name)
);

CREATE TABLE IF NOT EXISTS episode_tags (
    episode_id  TEXT NOT NULL REFERENCES episodes(id) ON DELETE CASCADE,
    tag         TEXT NOT NULL,
    PRIMARY KEY (episode_id, tag)
);

CREATE TABLE IF NOT EXISTS episode_state_changes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    episode_id  TEXT NOT NULL REFERENCES episodes(id) ON DELETE CASCADE,
    entity_kind TEXT NOT NULL,
    entity_name TEXT NOT NULL,
    change_type TEXT NOT NULL,
    from_value  TEXT,
    to_value    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_episodes_timestamp ON episodes(timestamp);
CREATE INDEX IF NOT EXISTS idx_episodes_session ON episodes(session_id);
CREATE INDEX IF NOT EXISTS idx_entities_lookup ON episode_entities(entity_kind, entity_name);
CREATE INDEX IF NOT EXISTS idx_tags_lookup ON episode_tags(tag);
CREATE INDEX IF NOT EXISTS idx_state_changes_entity ON episode_state_changes(entity_kind, entity_name);
"#),
    ("002_fts5", r#"
CREATE VIRTUAL TABLE IF NOT EXISTS episodes_fts USING fts5(
    title, summary, content='episodes', content_rowid='rowid'
);

-- Triggers to keep FTS index in sync
CREATE TRIGGER IF NOT EXISTS episodes_ai AFTER INSERT ON episodes BEGIN
    INSERT INTO episodes_fts(rowid, title, summary)
    VALUES (new.rowid, new.title, new.summary);
END;

CREATE TRIGGER IF NOT EXISTS episodes_ad AFTER DELETE ON episodes BEGIN
    INSERT INTO episodes_fts(episodes_fts, rowid, title, summary)
    VALUES ('delete', old.rowid, old.title, old.summary);
END;

CREATE TRIGGER IF NOT EXISTS episodes_au AFTER UPDATE ON episodes BEGIN
    INSERT INTO episodes_fts(episodes_fts, rowid, title, summary)
    VALUES ('delete', old.rowid, old.title, old.summary);
    INSERT INTO episodes_fts(rowid, title, summary)
    VALUES (new.rowid, new.title, new.summary);
END;
"#),
];
