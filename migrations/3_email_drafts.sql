-- Migration V003: Email Drafts
-- Adds email drafts table for storing pending email compositions

CREATE TABLE IF NOT EXISTS email_drafts (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    to_address TEXT NOT NULL,
    cc TEXT,
    subject TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

-- Indexes for draft queries
CREATE INDEX IF NOT EXISTS idx_email_drafts_user_id ON email_drafts(user_id);
CREATE INDEX IF NOT EXISTS idx_email_drafts_expires_at ON email_drafts(expires_at);
