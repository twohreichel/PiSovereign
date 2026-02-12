-- Suspicious activity tracking tables for persistent security monitoring
-- Replaces in-memory tracking for multi-instance support

CREATE TABLE IF NOT EXISTS security_violations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ip TEXT NOT NULL,
    category TEXT NOT NULL,
    threat_level TEXT NOT NULL,
    details TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_security_violations_ip ON security_violations(ip);
CREATE INDEX IF NOT EXISTS idx_security_violations_created_at ON security_violations(created_at);

CREATE TABLE IF NOT EXISTS ip_blocks (
    ip TEXT PRIMARY KEY,
    blocked_until TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_ip_blocks_blocked_until ON ip_blocks(blocked_until);
