-- Migration 10: Add metadata column to messages
-- Stores optional JSON metadata for messages (tool results, etc.)

ALTER TABLE messages ADD COLUMN metadata TEXT;
