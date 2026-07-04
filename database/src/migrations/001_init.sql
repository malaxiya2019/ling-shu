-- Lingshu 核心表初始化
-- 适用: SQLite (开发) + PostgreSQL (生产)

-- 0. 通用文档存储表 — 供 Database trait 的通用 CRUD 使用
CREATE TABLE IF NOT EXISTS documents (
    id          TEXT PRIMARY KEY,
    collection  TEXT NOT NULL,
    payload     TEXT NOT NULL DEFAULT '{}',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_documents_collection ON documents(collection);

-- 1. 用户表
CREATE TABLE IF NOT EXISTS users (
    id          TEXT PRIMARY KEY,
    username    TEXT NOT NULL UNIQUE,
    email       TEXT,
    password_hash TEXT NOT NULL,
    roles       TEXT NOT NULL DEFAULT '[]',
    is_active   INTEGER NOT NULL DEFAULT 1,
    metadata    TEXT DEFAULT '{}',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 2. 会话表
CREATE TABLE IF NOT EXISTS sessions (
    id          TEXT PRIMARY KEY,
    user_id     TEXT,
    token       TEXT NOT NULL,
    state       TEXT NOT NULL DEFAULT 'active',
    ip_address  TEXT,
    user_agent  TEXT,
    expires_at  TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    terminated_at TEXT
);

-- 3. 记忆表
CREATE TABLE IF NOT EXISTS memories (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    agent_id    TEXT,
    content     TEXT NOT NULL,
    content_type TEXT NOT NULL DEFAULT 'text',
    metadata    TEXT DEFAULT '{}',
    importance  REAL DEFAULT 0.0,
    ttl_seconds INTEGER,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at  TEXT
);

-- 4. 向量表
CREATE TABLE IF NOT EXISTS vectors (
    id          TEXT PRIMARY KEY,
    collection  TEXT NOT NULL,
    vector      BLOB NOT NULL,
    payload     TEXT DEFAULT '{}',
    dimensions  INTEGER NOT NULL,
    metadata    TEXT DEFAULT '{}',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 5. 事件表
CREATE TABLE IF NOT EXISTS events (
    id          TEXT PRIMARY KEY,
    topic       TEXT NOT NULL,
    session_id  TEXT,
    trace_id    TEXT,
    payload     TEXT NOT NULL DEFAULT '{}',
    source      TEXT,
    severity    TEXT NOT NULL DEFAULT 'info',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 6. 审计日志表
CREATE TABLE IF NOT EXISTS audit_logs (
    id          TEXT PRIMARY KEY,
    actor_id    TEXT NOT NULL,
    session_id  TEXT,
    trace_id    TEXT,
    resource    TEXT NOT NULL,
    action      TEXT NOT NULL,
    result      TEXT NOT NULL,
    ip_address  TEXT,
    details     TEXT DEFAULT '{}',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 7. 插件表
CREATE TABLE IF NOT EXISTS plugins (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    version     TEXT NOT NULL,
    description TEXT,
    permissions TEXT NOT NULL DEFAULT '[]',
    enabled     INTEGER NOT NULL DEFAULT 1,
    config      TEXT DEFAULT '{}',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 索引
CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_token ON sessions(token);
CREATE INDEX IF NOT EXISTS idx_memories_session_id ON memories(session_id);
CREATE INDEX IF NOT EXISTS idx_memories_agent_id ON memories(agent_id);
CREATE INDEX IF NOT EXISTS idx_vectors_collection ON vectors(collection);
CREATE INDEX IF NOT EXISTS idx_events_topic ON events(topic);
CREATE INDEX IF NOT EXISTS idx_events_trace_id ON events(trace_id);
CREATE INDEX IF NOT EXISTS idx_audit_logs_actor ON audit_logs(actor_id);
CREATE INDEX IF NOT EXISTS idx_audit_logs_resource ON audit_logs(resource);
