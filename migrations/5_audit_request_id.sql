-- Migration V005: Add request_id to audit_log for distributed tracing
--
-- This enables correlation of audit entries with HTTP requests across
-- the distributed system via X-Request-Id headers.

-- Add request_id column (UUID stored as TEXT, nullable for historical entries)
ALTER TABLE audit_log ADD COLUMN request_id TEXT;

-- Create index for efficient queries by request_id
CREATE INDEX IF NOT EXISTS idx_audit_log_request_id ON audit_log(request_id);
