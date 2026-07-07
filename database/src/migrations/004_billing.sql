-- Lingshu 计费/用量表
-- 适用: SQLite (主数据库)
-- 说明: 用量追踪与配额管理的持久化存储

-- 用量记录
CREATE TABLE IF NOT EXISTS usage_records (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL,
    resource    TEXT NOT NULL,
    amount      REAL NOT NULL DEFAULT 0.0,
    unit        TEXT NOT NULL DEFAULT 'tokens',
    metadata    TEXT DEFAULT '{}',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_usage_user ON usage_records(user_id);
CREATE INDEX IF NOT EXISTS idx_usage_resource ON usage_records(resource);

-- 配额配置
CREATE TABLE IF NOT EXISTS quota_configs (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL UNIQUE,
    plan_name   TEXT NOT NULL DEFAULT 'free',
    max_tokens  INTEGER NOT NULL DEFAULT 1000000,
    max_requests INTEGER NOT NULL DEFAULT 1000,
    reset_interval_seconds INTEGER NOT NULL DEFAULT 86400,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
