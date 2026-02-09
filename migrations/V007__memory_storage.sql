-- Migration V007: Memory Storage for AI Knowledge Base
-- Creates tables for storing AI memories with vector embeddings for semantic search

-- Main memories table
CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY,
    -- User who owns this memory
    user_id TEXT NOT NULL,
    -- Optional conversation this memory originated from
    conversation_id TEXT,
    -- Encrypted content of the memory
    content TEXT NOT NULL,
    -- Summary/title for quick reference
    summary TEXT NOT NULL,
    -- Vector embedding as JSON array (for semantic search)
    -- Using JSON since sqlite-vec may not be available everywhere
    embedding TEXT,
    -- Importance score (0.0 - 1.0) for relevance ranking and decay
    importance REAL NOT NULL DEFAULT 0.5,
    -- Type of memory: 'fact', 'preference', 'tool_result', 'correction', 'context'
    memory_type TEXT NOT NULL CHECK(memory_type IN ('fact', 'preference', 'tool_result', 'correction', 'context')),
    -- JSON array of tags for categorical filtering
    tags TEXT NOT NULL DEFAULT '[]',
    -- Timestamps
    created_at TEXT NOT NULL,
    accessed_at TEXT NOT NULL,
    -- Access statistics
    access_count INTEGER NOT NULL DEFAULT 0,
    -- Foreign key constraints
    FOREIGN KEY (user_id) REFERENCES user_profiles(id) ON DELETE CASCADE,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE SET NULL
);

-- Indexes for efficient queries
-- User-based queries (most common access pattern)
CREATE INDEX IF NOT EXISTS idx_memories_user ON memories(user_id);
-- Type-based filtering
CREATE INDEX IF NOT EXISTS idx_memories_type ON memories(user_id, memory_type);
-- Importance-based sorting and decay queries
CREATE INDEX IF NOT EXISTS idx_memories_importance ON memories(user_id, importance DESC);
-- Timestamp-based queries for decay and cleanup
CREATE INDEX IF NOT EXISTS idx_memories_accessed ON memories(accessed_at);
CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at);
-- Conversation-based lookup
CREATE INDEX IF NOT EXISTS idx_memories_conversation ON memories(conversation_id) WHERE conversation_id IS NOT NULL;

-- Full-text search index for content (using FTS5)
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    id UNINDEXED,
    summary,
    content,
    tags,
    content='memories',
    content_rowid='rowid',
    tokenize='porter unicode61'
);

-- Triggers to keep FTS index in sync
CREATE TRIGGER IF NOT EXISTS memories_fts_insert AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(rowid, id, summary, content, tags)
    VALUES (NEW.rowid, NEW.id, NEW.summary, NEW.content, NEW.tags);
END;

CREATE TRIGGER IF NOT EXISTS memories_fts_delete AFTER DELETE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, id, summary, content, tags)
    VALUES ('delete', OLD.rowid, OLD.id, OLD.summary, OLD.content, OLD.tags);
END;

CREATE TRIGGER IF NOT EXISTS memories_fts_update AFTER UPDATE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, id, summary, content, tags)
    VALUES ('delete', OLD.rowid, OLD.id, OLD.summary, OLD.content, OLD.tags);
    INSERT INTO memories_fts(rowid, id, summary, content, tags)
    VALUES (NEW.rowid, NEW.id, NEW.summary, NEW.content, NEW.tags);
END;

-- Memory embeddings table for vector search
-- Stored separately for efficient vector operations
CREATE TABLE IF NOT EXISTS memory_embeddings (
    memory_id TEXT PRIMARY KEY,
    -- Embedding as blob (more efficient than JSON for large vectors)
    embedding BLOB NOT NULL,
    -- Number of dimensions (for validation)
    dimensions INTEGER NOT NULL,
    -- Model used to generate the embedding
    model TEXT NOT NULL,
    -- When the embedding was generated
    created_at TEXT NOT NULL,
    FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE
);

-- Index for joining with memories
CREATE INDEX IF NOT EXISTS idx_memory_embeddings_memory ON memory_embeddings(memory_id);
