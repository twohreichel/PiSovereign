-- Migration V009: Conversation Source Tracking
-- Extends conversations table to track message source (HTTP, WhatsApp, Signal)
-- and phone numbers for messenger conversations

-- Add source column to track where conversations originate from
-- Values: 'http' (web/API), 'whatsapp', 'signal'
ALTER TABLE conversations ADD COLUMN source TEXT NOT NULL DEFAULT 'http'
    CHECK(source IN ('http', 'whatsapp', 'signal'));

-- Add phone number for messenger conversations (nullable for HTTP)
-- Stored in E.164 format (e.g., "+1234567890")
ALTER TABLE conversations ADD COLUMN phone_number TEXT;

-- Index for fast lookup of messenger conversations by phone number
-- This is the primary access pattern for continuing messenger conversations
CREATE INDEX IF NOT EXISTS idx_conversations_phone
    ON conversations(phone_number, source)
    WHERE phone_number IS NOT NULL;

-- Index for filtering by source (useful for analytics and cleanup)
CREATE INDEX IF NOT EXISTS idx_conversations_source
    ON conversations(source);

-- Index for cleanup queries based on last update time
CREATE INDEX IF NOT EXISTS idx_conversations_updated
    ON conversations(updated_at);
