-- Lingshu 审计日志扩展表
-- 适用: SQLite (主数据库)
-- 说明: 补充审计日志的详细事件存储，超出 001_init 中 audit_logs 基础结构的部分

-- 审计事件详情 (支持更丰富的审计追踪)
CREATE TABLE IF NOT EXISTS audit_events (
    id          TEXT PRIMARY KEY,
    log_id      TEXT NOT NULL REFERENCES audit_logs(id),
    event_type  TEXT NOT NULL,
    severity    TEXT NOT NULL DEFAULT 'info',
    delta       TEXT NOT NULL DEFAULT '{}',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_audit_events_type ON audit_events(event_type);
CREATE INDEX IF NOT EXISTS idx_audit_events_log ON audit_events(log_id);
