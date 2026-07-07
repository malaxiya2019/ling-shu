-- Lingshu 知识图谱缓存表
-- 适用: SQLite (单独文件: graphs.db)
-- 说明: 项目知识图谱的持久化缓存，通过 GraphStore 管理

CREATE TABLE IF NOT EXISTS graph_cache (
    project_name TEXT PRIMARY KEY NOT NULL,
    graph_json   TEXT NOT NULL,
    node_count   INTEGER NOT NULL DEFAULT 0,
    edge_count   INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);
