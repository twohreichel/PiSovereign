-- Migration V006: Retry Queue
-- Creates tables for persistent retry queue with exponential backoff

-- Retry queue table for failed webhook/message deliveries
CREATE TABLE IF NOT EXISTS retry_queue (
    id TEXT PRIMARY KEY,
    -- Type of operation: 'webhook', 'message', 'email', etc.
    operation_type TEXT NOT NULL,
    -- JSON payload to retry
    payload TEXT NOT NULL,
    -- Target endpoint or recipient
    target TEXT NOT NULL,
    -- Number of retry attempts made
    attempt_count INTEGER NOT NULL DEFAULT 0,
    -- Maximum retries before marked as failed
    max_retries INTEGER NOT NULL DEFAULT 5,
    -- Next scheduled retry time (ISO8601)
    next_retry_at TEXT NOT NULL,
    -- Current status: 'pending', 'in_progress', 'completed', 'failed', 'cancelled'
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'in_progress', 'completed', 'failed', 'cancelled')),
    -- Last error message if failed
    last_error TEXT,
    -- Original creation time
    created_at TEXT NOT NULL,
    -- Last update time
    updated_at TEXT NOT NULL,
    -- Correlation ID for tracing
    correlation_id TEXT,
    -- User/tenant context (optional)
    user_id TEXT,
    tenant_id TEXT
);

-- Indexes for efficient queue operations
CREATE INDEX IF NOT EXISTS idx_retry_queue_status ON retry_queue(status);
CREATE INDEX IF NOT EXISTS idx_retry_queue_next_retry ON retry_queue(next_retry_at) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_retry_queue_operation ON retry_queue(operation_type);
CREATE INDEX IF NOT EXISTS idx_retry_queue_created ON retry_queue(created_at);
CREATE INDEX IF NOT EXISTS idx_retry_queue_user ON retry_queue(user_id) WHERE user_id IS NOT NULL;

-- Dead letter queue for permanently failed items
CREATE TABLE IF NOT EXISTS dead_letter_queue (
    id TEXT PRIMARY KEY,
    original_id TEXT NOT NULL,
    operation_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    target TEXT NOT NULL,
    attempt_count INTEGER NOT NULL,
    last_error TEXT,
    created_at TEXT NOT NULL,
    failed_at TEXT NOT NULL,
    correlation_id TEXT,
    user_id TEXT,
    tenant_id TEXT
);

CREATE INDEX IF NOT EXISTS idx_dlq_created ON dead_letter_queue(created_at);
CREATE INDEX IF NOT EXISTS idx_dlq_operation ON dead_letter_queue(operation_type);
