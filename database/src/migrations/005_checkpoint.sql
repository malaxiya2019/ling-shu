-- ── Agent Checkpoints ──────────────────────────────
-- 持久化 Agent 状态快照和检查点，支持 SQLite + PostgreSQL

CREATE TABLE IF NOT EXISTS agent_checkpoints (
    id          TEXT PRIMARY KEY,
    agent_id    TEXT NOT NULL,
    session_id  TEXT NOT NULL,
    state_json  TEXT NOT NULL,
    metadata    TEXT NOT NULL DEFAULT '{}',
    version     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at  TEXT
);

CREATE INDEX IF NOT EXISTS idx_checkpoints_agent_id ON agent_checkpoints(agent_id);
CREATE INDEX IF NOT EXISTS idx_checkpoints_session_id ON agent_checkpoints(session_id);
CREATE INDEX IF NOT EXISTS idx_checkpoints_created_at ON agent_checkpoints(created_at);

-- ── Conversation Embeddings ─────────────────────────
-- 对话向量化存储 (pgvector 兼容)

CREATE TABLE IF NOT EXISTS conversation_embeddings (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    role        TEXT NOT NULL,
    content     TEXT NOT NULL,
    embedding   BLOB,
    model       TEXT NOT NULL DEFAULT 'text-embedding-ada-002',
    token_count INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_embeddings_session_id ON conversation_embeddings(session_id);
CREATE INDEX IF NOT EXISTS idx_embeddings_created_at ON conversation_embeddings(created_at);

-- ── Agent State Machine ─────────────────────────────
-- Agent 状态机持久化

CREATE TABLE IF NOT EXISTS agent_states (
    id          TEXT PRIMARY KEY,
    agent_id    TEXT NOT NULL UNIQUE,
    status      TEXT NOT NULL DEFAULT 'idle',
    phase       TEXT NOT NULL DEFAULT 'init',
    state_machine_json TEXT NOT NULL DEFAULT '{}',
    variables   TEXT NOT NULL DEFAULT '{}',
    history     TEXT NOT NULL DEFAULT '[]',
    version     INTEGER NOT NULL DEFAULT 1,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_agent_states_status ON agent_states(status);
