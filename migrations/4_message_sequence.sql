-- Migration V004: Add sequence_number to messages for reliable ordering
--
-- This enables incremental persistence - only messages with sequence_number > max(existing)
-- need to be persisted during sync operations, instead of relying on created_at timestamps.

-- Add sequence_number column with default 0 for existing rows
ALTER TABLE messages ADD COLUMN sequence_number INTEGER NOT NULL DEFAULT 0;

-- Create index for efficient ordering queries
CREATE INDEX idx_messages_sequence ON messages(conversation_id, sequence_number);

-- Update existing messages to have sequence numbers based on creation order
-- Using a CTE to assign sequential numbers per conversation
WITH numbered_messages AS (
    SELECT 
        id,
        conversation_id,
        ROW_NUMBER() OVER (PARTITION BY conversation_id ORDER BY created_at, id) as seq
    FROM messages
)
UPDATE messages
SET sequence_number = (
    SELECT seq FROM numbered_messages 
    WHERE numbered_messages.id = messages.id
);
