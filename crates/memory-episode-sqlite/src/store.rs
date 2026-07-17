//! SQLiteEpisodeStore — SQLite 持久化的 Episode 存储实现。

use async_trait::async_trait;
use lingshu_core::{LsError, LsResult};
use lingshu_memory_episode::{
    EntityRef, Episode, EpisodeId, EpisodeQuery, EpisodeRepository, QueryStats, SortOrder, StateChange,
};
use uuid::Uuid;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;

use crate::migrations::MIGRATIONS;

/// SQLiteEpisodeStore — SQLite 持久化的事件存储。
///
/// 支持 WAL 模式、自动迁移、FTS5 全文搜索。
///
/// # 示例
///
/// ```rust,ignore
/// let store = SQLiteEpisodeStore::in_memory().await.unwrap();
/// let id = store.store(Episode::new("事件", "描述")).await.unwrap();
/// ```
#[derive(Clone)]
pub struct SQLiteEpisodeStore {
    conn: Arc<Mutex<Connection>>,
}

impl SQLiteEpisodeStore {
    /// 创建或打开数据库文件。
    pub fn new(path: impl AsRef<Path>) -> LsResult<Self> {
        let conn = Self::open_and_migrate(path)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 从现有连接创建（用于测试/内存数据库）。
    pub fn from_connection(conn: Connection) -> LsResult<Self> {
        Self::run_migrations(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 创建内存数据库。
    pub fn in_memory() -> LsResult<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| LsError::Internal(format!("sqlite in-memory open failed: {e}")))?;
        Self::from_connection(conn)
    }

    // ── 内部方法 ──

    fn open_and_migrate(path: impl AsRef<Path>) -> LsResult<Connection> {
        let conn = Connection::open(path)
            .map_err(|e| LsError::Internal(format!("sqlite open failed: {e}")))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;",
        )
        .map_err(|e| LsError::Internal(format!("sqlite pragma failed: {e}")))?;
        Self::run_migrations(&conn)?;
        Ok(conn)
    }

    fn run_migrations(conn: &Connection) -> LsResult<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _episode_migrations (
                version TEXT PRIMARY KEY NOT NULL,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .map_err(|e| LsError::Internal(format!("migrations table creation failed: {e}")))?;

        for (name, sql) in MIGRATIONS {
            let already_applied: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM _episode_migrations WHERE version = ?1",
                    params![name],
                    |row| row.get(0),
                )
                .unwrap_or(false);

            if !already_applied {
                conn.execute_batch(sql)
                    .map_err(|e| LsError::Internal(format!("migration {name} failed: {e}")))?;
                conn.execute(
                    "INSERT INTO _episode_migrations (version) VALUES (?1)",
                    params![name],
                )
                .map_err(|e| LsError::Internal(format!("migration tracking failed: {e}")))?;
                debug!(migration = %name, "applied");
            }
        }

        Ok(())
    }
}

#[async_trait]
impl EpisodeRepository for SQLiteEpisodeStore {
    async fn store(&self, episode: Episode) -> LsResult<EpisodeId> {
        let id = episode.id;
        let entities = episode.entities.clone();
        let tags = episode.tags.clone();
        let state_changes = episode.state_changes.clone();
        let metadata_json = serde_json::to_string(&episode.metadata)
            .unwrap_or_else(|_| "{}".to_string());

        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO episodes (id, title, summary, timestamp, session_id, source_ref, importance, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id.to_string(),
                episode.title,
                episode.summary,
                episode.timestamp.to_rfc3339(),
                episode.session_id,
                episode.source_ref,
                0.5_f64,
                metadata_json,
            ],
        )
        .map_err(|e| LsError::Internal(format!("episode store failed: {e}")))?;

        // 清理旧索引
        conn.execute("DELETE FROM episode_entities WHERE episode_id = ?1", params![id.to_string()])
            .ok();
        conn.execute("DELETE FROM episode_tags WHERE episode_id = ?1", params![id.to_string()])
            .ok();
        conn.execute("DELETE FROM episode_state_changes WHERE episode_id = ?1", params![id.to_string()])
            .ok();

        // 存储实体
        for entity in &entities {
            conn.execute(
                "INSERT INTO episode_entities (episode_id, entity_kind, entity_name) VALUES (?1, ?2, ?3)",
                params![id.to_string(), entity.kind, entity.name],
            )
            .map_err(|e| LsError::Internal(format!("entity store failed: {e}")))?;
        }

        // 存储标签
        for tag in &tags {
            conn.execute(
                "INSERT INTO episode_tags (episode_id, tag) VALUES (?1, ?2)",
                params![id.to_string(), tag],
            )
            .map_err(|e| LsError::Internal(format!("tag store failed: {e}")))?;
        }

        // 存储状态变更
        for sc in &state_changes {
            conn.execute(
                "INSERT INTO episode_state_changes (episode_id, entity_kind, entity_name, change_type, from_value, to_value)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id.to_string(),
                    sc.entity.kind,
                    sc.entity.name,
                    sc.change_type,
                    sc.from,
                    sc.to,
                ],
            )
            .map_err(|e| LsError::Internal(format!("state change store failed: {e}")))?;
        }

        debug!(episode_id = %id, "sqlite episode stored");
        Ok(id)
    }

    async fn store_batch(&self, episodes: Vec<Episode>) -> LsResult<Vec<EpisodeId>> {
        let mut ids = Vec::with_capacity(episodes.len());
        for ep in episodes {
            let id = self.store(ep).await?;
            ids.push(id);
        }
        Ok(ids)
    }

    async fn get(&self, id: EpisodeId) -> LsResult<Episode> {
        let conn = self.conn.lock().await;
        let id_str = id.to_string();

        let mut stmt = conn.prepare(
            "SELECT id, title, summary, timestamp, session_id, source_ref, metadata
             FROM episodes WHERE id = ?1",
        )
        .map_err(|e| LsError::Internal(format!("prepare failed: {e}")))?;

        let episode = stmt.query_row(params![id_str], |row| {
            let id_str: String = row.get(0)?;
            let title: String = row.get(1)?;
            let summary: String = row.get(2)?;
            let ts_str: String = row.get(3)?;
            let session_id: Option<String> = row.get(4)?;
            let source_ref: Option<String> = row.get(5)?;
            let metadata_str: String = row.get(6)?;

            let timestamp = chrono::DateTime::parse_from_rfc3339(&ts_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let episode_id = Uuid::parse_str(&id_str).ok().map(EpisodeId::from_uuid)
                .unwrap_or_default();

            let metadata: std::collections::HashMap<String, String> =
                serde_json::from_str(&metadata_str).unwrap_or_default();

            Ok((episode_id, title, summary, timestamp, session_id, source_ref, metadata))
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                LsError::NotFound(format!("episode {id} not found"))
            }
            other => LsError::Internal(format!("get episode failed: {other}")),
        })?;

        let (episode_id, title, summary, timestamp, session_id, source_ref, metadata) = episode;

        // 加载实体
        let mut entity_stmt = conn.prepare(
            "SELECT entity_kind, entity_name FROM episode_entities WHERE episode_id = ?1",
        )
        .map_err(|e| LsError::Internal(format!("prepare entities failed: {e}")))?;

        let entities: Vec<EntityRef> = entity_stmt
            .query_map(params![id_str], |row| {
                let kind: String = row.get(0)?;
                let name: String = row.get(1)?;
                Ok(EntityRef::new(kind, name))
            })
            .map_err(|e| LsError::Internal(format!("query entities failed: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        // 加载标签
        let mut tag_stmt = conn.prepare(
            "SELECT tag FROM episode_tags WHERE episode_id = ?1",
        )
        .map_err(|e| LsError::Internal(format!("prepare tags failed: {e}")))?;

        let tags: Vec<String> = tag_stmt
            .query_map(params![id_str], |row| row.get(0))
            .map_err(|e| LsError::Internal(format!("query tags failed: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        // 加载状态变更
        let mut sc_stmt = conn.prepare(
            "SELECT entity_kind, entity_name, change_type, from_value, to_value
             FROM episode_state_changes WHERE episode_id = ?1 ORDER BY id",
        )
        .map_err(|e| LsError::Internal(format!("prepare state changes failed: {e}")))?;

        let state_changes: Vec<StateChange> = sc_stmt
            .query_map(params![id_str], |row| {
                let kind: String = row.get(0)?;
                let name: String = row.get(1)?;
                let change_type: String = row.get(2)?;
                let from_val: Option<String> = row.get(3)?;
                let to_val: String = row.get(4)?;
                Ok(StateChange::new(
                    EntityRef::new(kind, name),
                    change_type,
                    from_val,
                    to_val,
                ))
            })
            .map_err(|e| LsError::Internal(format!("query state changes failed: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(Episode {
            id: episode_id,
            title,
            summary,
            timestamp,
            entities,
            tags,
            state_changes,
            session_id,
            source_ref,
            metadata,
        })
    }

    async fn query(&self, query: EpisodeQuery) -> LsResult<Vec<Episode>> {
        let (results, _) = self.query_with_stats(query).await?;
        Ok(results)
    }

    async fn query_with_stats(&self, query: EpisodeQuery) -> LsResult<(Vec<Episode>, QueryStats)> {
        let start = std::time::Instant::now();
        let conn = self.conn.lock().await;

        let mut sql = String::from(
            "SELECT DISTINCT e.id, e.title, e.summary, e.timestamp, e.session_id, e.source_ref, e.metadata
             FROM episodes e"
        );
        let mut conditions: Vec<String> = Vec::new();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        // 实体过滤
        if !query.entities.is_empty() {
            sql.push_str(" JOIN episode_entities ent ON ent.episode_id = e.id");
            let entity_conditions: Vec<String> = query.entities.iter().map(|_| {
                "(ent.entity_kind = ? AND ent.entity_name = ?)".to_string()
            }).collect();
            conditions.push(format!("({})", entity_conditions.join(" OR ")));
            for entity in &query.entities {
                param_values.push(Box::new(entity.kind.clone()) as Box<dyn rusqlite::types::ToSql>);
                param_values.push(Box::new(entity.name.clone()) as Box<dyn rusqlite::types::ToSql>);
            }
        }

        // 标签过滤
        if !query.tags.is_empty() {
            sql.push_str(" JOIN episode_tags tg ON tg.episode_id = e.id");
            let tag_conditions: Vec<String> = query.tags.iter().map(|_| "tg.tag = ?".to_string()).collect();
            conditions.push(format!("({})", tag_conditions.join(" OR ")));
            for tag in &query.tags {
                param_values.push(Box::new(tag.clone()) as Box<dyn rusqlite::types::ToSql>);
            }
        }

        // 时间范围
        if let Some(from) = query.time_from {
            conditions.push("e.timestamp >= ?".to_string());
            param_values.push(Box::new(from.to_rfc3339()) as Box<dyn rusqlite::types::ToSql>);
        }
        if let Some(to) = query.time_to {
            conditions.push("e.timestamp <= ?".to_string());
            param_values.push(Box::new(to.to_rfc3339()) as Box<dyn rusqlite::types::ToSql>);
        }

        // 会话过滤
        if let Some(ref sid) = query.session_id {
            conditions.push("e.session_id = ?".to_string());
            param_values.push(Box::new(sid.clone()) as Box<dyn rusqlite::types::ToSql>);
        }

        // 关键词搜索（使用 LIKE，兼容所有语言）
        let use_search = query.search_text.as_ref().is_some_and(|t| !t.is_empty());
        if use_search {
            if let Some(ref text) = query.search_text {
                // 过滤特殊字符
                let escaped: String = text
                    .chars()
                    .filter(|c| !"*^$()[]{}!~@#&".contains(*c))
                    .collect();
                if !escaped.is_empty() {
                    let pattern = format!("%{}%", escaped);
                    conditions.push(
                        "(e.title LIKE ? OR e.summary LIKE ?)".to_string()
                    );
                    param_values.push(Box::new(pattern.clone()) as Box<dyn rusqlite::types::ToSql>);
                    param_values.push(Box::new(pattern) as Box<dyn rusqlite::types::ToSql>);
                }
            }
        }
        // 组装 WHERE
        if !conditions.is_empty() {
            sql.push_str(&format!(" WHERE {}", conditions.join(" AND ")));
        }

        // 排序
        match query.sort_order {
            SortOrder::Ascending => sql.push_str(" ORDER BY e.timestamp ASC"),
            SortOrder::Descending => sql.push_str(" ORDER BY e.timestamp DESC"),
        }

        // 分页
        sql.push_str(" LIMIT ? OFFSET ?");
        let limit = query.limit;
        let offset = query.offset;
        param_values.push(Box::new(limit as i64) as Box<dyn rusqlite::types::ToSql>);
        param_values.push(Box::new(offset as i64) as Box<dyn rusqlite::types::ToSql>);

        // 执行查询
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)
            .map_err(|e| LsError::Internal(format!("prepare query failed: {e}\nSQL: {sql}")))?;

        let episode_rows: Vec<(String, String, String, String, Option<String>, Option<String>, String)> = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .map_err(|e| LsError::Internal(format!("query failed: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        // 对每个 episode 加载实体、标签、状态变更
        let mut episodes = Vec::with_capacity(episode_rows.len());
        for (id_str, title, summary, ts_str, session_id, source_ref, metadata_str) in episode_rows {
            let timestamp = match chrono::DateTime::parse_from_rfc3339(&ts_str) {
                Ok(dt) => dt.with_timezone(&chrono::Utc),
                Err(_) => continue,
            };

            let episode_id = match Uuid::parse_str(&id_str) {
                Ok(u) => EpisodeId::from_uuid(u),
                Err(_) => continue,
            };

            let metadata: std::collections::HashMap<String, String> =
                serde_json::from_str(&metadata_str).unwrap_or_default();

            // Load entities
            let mut entity_stmt = conn.prepare(
                "SELECT entity_kind, entity_name FROM episode_entities WHERE episode_id = ?1",
            )
            .map_err(|e| LsError::Internal(format!("prepare entities failed: {e}")))?;

            let entities: Vec<EntityRef> = entity_stmt
                .query_map(params![id_str], |row| {
                    Ok(EntityRef::new(row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| LsError::Internal(format!("query entities failed: {e}")))?
                .filter_map(|r| r.ok())
                .collect();

            // Load tags
            let mut tag_stmt = conn.prepare(
                "SELECT tag FROM episode_tags WHERE episode_id = ?1",
            )
            .map_err(|e| LsError::Internal(format!("prepare tags failed: {e}")))?;

            let tags: Vec<String> = tag_stmt
                .query_map(params![id_str], |row| row.get::<_, String>(0))
                .map_err(|e| LsError::Internal(format!("query tags failed: {e}")))?
                .filter_map(|r| r.ok())
                .collect();

            // Load state changes
            let mut sc_stmt = conn.prepare(
                "SELECT entity_kind, entity_name, change_type, from_value, to_value
                 FROM episode_state_changes WHERE episode_id = ?1 ORDER BY id",
            )
            .map_err(|e| LsError::Internal(format!("prepare state changes failed: {e}")))?;

            let state_changes: Vec<StateChange> = sc_stmt
                .query_map(params![id_str], |row| {
                    Ok(StateChange::new(
                        EntityRef::new(row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                })
                .map_err(|e| LsError::Internal(format!("query state changes failed: {e}")))?
                .filter_map(|r| r.ok())
                .collect();

            episodes.push(Episode {
                id: episode_id,
                title,
                summary,
                timestamp,
                entities,
                tags,
                state_changes,
                session_id,
                source_ref,
                metadata,
            });
        }

        let total_matched = episodes.len();
        let elapsed = start.elapsed().as_millis() as u64;

        let returned = episodes.len();
        Ok((
            episodes,
            QueryStats {
                total_matched,
                returned,
                query_time_ms: elapsed,
            },
        ))
    }

    async fn delete(&self, id: EpisodeId) -> LsResult<bool> {
        let conn = self.conn.lock().await;
        // CASCADE will clean up related rows
        let affected = conn
            .execute("DELETE FROM episodes WHERE id = ?1", params![id.to_string()])
            .map_err(|e| LsError::Internal(format!("delete failed: {e}")))?;
        Ok(affected > 0)
    }

    async fn delete_by_entity(&self, entity: &str) -> LsResult<usize> {
        let conn = self.conn.lock().await;
        let affected = conn
            .execute(
                "DELETE FROM episodes WHERE id IN (
                    SELECT episode_id FROM episode_entities
                    WHERE entity_kind || ':' || entity_name = ?1
                )",
                params![entity],
            )
            .map_err(|e| LsError::Internal(format!("delete by entity failed: {e}")))?;
        Ok(affected)
    }

    async fn count(&self) -> LsResult<usize> {
        let conn = self.conn.lock().await;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM episodes", [], |row| row.get(0))
            .map_err(|e| LsError::Internal(format!("count failed: {e}")))?;
        Ok(count as usize)
    }

    async fn list_entities(&self) -> LsResult<Vec<String>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn
            .prepare("SELECT DISTINCT entity_kind || ':' || entity_name FROM episode_entities ORDER BY 1")
            .map_err(|e| LsError::Internal(format!("list entities failed: {e}")))?;

        let entities: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| LsError::Internal(format!("query entities failed: {e}")))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(entities)
    }

    async fn clear(&self) -> LsResult<()> {
        let conn = self.conn.lock().await;
        conn.execute_batch(
            "DELETE FROM episode_state_changes;
             DELETE FROM episode_entities;
             DELETE FROM episode_tags;
             DELETE FROM episodes;",
        )
        .map_err(|e| LsError::Internal(format!("clear failed: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_memory_episode::{EntityRef, Episode, EpisodeQuery, SortOrder};
    use chrono::Duration;

    fn make_store() -> SQLiteEpisodeStore {
        SQLiteEpisodeStore::in_memory().unwrap()
    }

    fn make_episode(
        title: &str,
        summary: &str,
        days_ago: i64,
        entities: Vec<(&str, &str)>,
        tags: Vec<&str>,
    ) -> Episode {
        let mut ep = Episode::new(title, summary, chrono::Utc::now() - Duration::days(days_ago));
        for (kind, name) in entities {
            ep = ep.with_entity(EntityRef::new(kind, name));
        }
        for tag in tags {
            ep = ep.with_tag(tag);
        }
        ep
    }

    #[tokio::test]
    async fn test_store_and_get() {
        let store = make_store();
        let ep = make_episode("测试事件", "这是一个测试", 1, vec![("project", "测试项目")], vec!["test"]);
        let id = store.store(ep).await.unwrap();
        let retrieved = store.get(id).await.unwrap();
        assert_eq!(retrieved.title, "测试事件");
        assert_eq!(retrieved.entities.len(), 1);
        assert_eq!(retrieved.tags.len(), 1);
    }

    #[tokio::test]
    async fn test_query_by_entity() {
        let store = make_store();
        store.store(make_episode("事件A", "项目A启动", 5, vec![("project", "项目A")], vec!["launch"])).await.unwrap();
        store.store(make_episode("事件B", "项目B启动", 3, vec![("project", "项目B")], vec!["launch"])).await.unwrap();

        let results = store.query(
            EpisodeQuery::default().with_entity(EntityRef::new("project", "项目A")),
        ).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "事件A");
    }

    #[tokio::test]
    async fn test_query_by_tag() {
        let store = make_store();
        store.store(make_episode("里程碑1", "第一个里程碑", 10, vec![], vec!["milestone"])).await.unwrap();
        store.store(make_episode("日常更新", "日常", 1, vec![], vec!["daily"])).await.unwrap();

        let results = store.query(
            EpisodeQuery::default().with_tag("milestone"),
        ).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_search_text() {
        let store = make_store();
        store.store(make_episode("项目A暂停", "因为供应商问题暂停", 5, vec![], vec![])).await.unwrap();
        store.store(make_episode("项目B完成", "全部完成", 3, vec![], vec![])).await.unwrap();

        let results = store.query(
            EpisodeQuery::default().with_search("暂停"),
        ).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "项目A暂停");
    }

    #[tokio::test]
    async fn test_query_time_range() {
        let store = make_store();
        store.store(make_episode("旧事件", "很旧", 100, vec![], vec![])).await.unwrap();
        store.store(make_episode("新事件", "最近", 1, vec![], vec![])).await.unwrap();

        let from = chrono::Utc::now() - Duration::days(7);
        let results = store.query(
            EpisodeQuery::default().with_time_range(from, chrono::Utc::now()),
        ).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "新事件");
    }

    #[tokio::test]
    async fn test_sort_order() {
        let store = make_store();
        store.store(make_episode("最早", "最早的事件", 10, vec![], vec![])).await.unwrap();
        store.store(make_episode("最新", "最新的事件", 1, vec![], vec![])).await.unwrap();

        let asc = store.query(
            EpisodeQuery::default().with_sort(SortOrder::Ascending),
        ).await.unwrap();
        assert_eq!(asc[0].title, "最早");

        let desc = store.query(
            EpisodeQuery::default().with_sort(SortOrder::Descending),
        ).await.unwrap();
        assert_eq!(desc[0].title, "最新");
    }

    #[tokio::test]
    async fn test_delete() {
        let store = make_store();
        let ep = make_episode("待删除", "将被删除", 1, vec![("project", "项目X")], vec!["temp"]);
        let id = store.store(ep).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        let deleted = store.delete(id).await.unwrap();
        assert!(deleted);
        assert_eq!(store.count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_clear() {
        let store = make_store();
        store.store(make_episode("事件1", "描述1", 1, vec![], vec![])).await.unwrap();
        store.store(make_episode("事件2", "描述2", 2, vec![], vec![])).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 2);

        store.clear().await.unwrap();
        assert_eq!(store.count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_list_entities() {
        let store = make_store();
        store.store(make_episode("事件1", "desc", 1, vec![("project", "项目A"), ("person", "张三")], vec![])).await.unwrap();
        store.store(make_episode("事件2", "desc", 1, vec![("project", "项目B"), ("person", "李四")], vec![])).await.unwrap();

        let entities = store.list_entities().await.unwrap();
        assert!(entities.contains(&"person:张三".to_string()));
        assert!(entities.contains(&"project:项目A".to_string()));
        assert!(entities.contains(&"project:项目B".to_string()));
    }

    #[tokio::test]
    async fn test_store_batch() {
        let store = make_store();
        let episodes = vec![
            make_episode("事件1", "描述1", 1, vec![], vec![]),
            make_episode("事件2", "描述2", 2, vec![], vec![]),
            make_episode("事件3", "描述3", 3, vec![], vec![]),
        ];
        let ids = store.store_batch(episodes).await.unwrap();
        assert_eq!(ids.len(), 3);
        assert_eq!(store.count().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn test_delete_by_entity() {
        let store = make_store();
        store.store(make_episode("事件A", "desc", 1, vec![("project", "项目X")], vec![])).await.unwrap();
        store.store(make_episode("事件B", "desc", 1, vec![("project", "项目X")], vec![])).await.unwrap();
        store.store(make_episode("事件C", "desc", 1, vec![("project", "项目Y")], vec![])).await.unwrap();

        let deleted = store.delete_by_entity("project:项目X").await.unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(store.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_store_with_state_changes() {
        let store = make_store();
        let mut ep = Episode::new("状态变更", "状态从active变为paused", chrono::Utc::now());
        ep = ep.with_entity(EntityRef::new("project", "项目X"));
        ep = ep.with_state_change(StateChange::new(
            EntityRef::new("project", "项目X"),
            "status",
            Some("active".to_string()),
            "paused",
        ));
        let id = store.store(ep).await.unwrap();

        let retrieved = store.get(id).await.unwrap();
        assert_eq!(retrieved.state_changes.len(), 1);
        assert_eq!(retrieved.state_changes[0].change_type, "status");
        assert_eq!(retrieved.state_changes[0].from.as_deref(), Some("active"));
        assert_eq!(retrieved.state_changes[0].to, "paused");
    }

    #[tokio::test]
    async fn test_persistence_across_connections() {
        // Test that data persists when using same file
        let tmp = std::env::temp_dir().join(format!("test_episode_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&tmp);

        {
            let store = SQLiteEpisodeStore::new(&tmp).unwrap();
            store.store(make_episode("持久化测试", "应该能保存到文件", 1, vec![("project", "持久化项目")], vec!["test"])).await.unwrap();
            assert_eq!(store.count().await.unwrap(), 1);
        }

        // Reopen
        {
            let store = SQLiteEpisodeStore::new(&tmp).unwrap();
            assert_eq!(store.count().await.unwrap(), 1);

            let results = store.query(
                EpisodeQuery::default().with_entity(EntityRef::new("project", "持久化项目")),
            ).await.unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].title, "持久化测试");
        }

        let _ = std::fs::remove_file(&tmp);
    }
}
