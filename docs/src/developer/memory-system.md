# AI Memory System

PiSovereign includes a persistent AI memory system that enables your assistant to remember facts, preferences, and past interactions. This creates a more personalized and contextually aware experience.

## Overview

The memory system provides:

- **Persistent Storage**: All interactions can be stored in an encrypted SQLite database
- **Semantic Search (RAG)**: Retrieve relevant memories based on meaning, not just keywords
- **Automatic Learning**: The AI learns from conversations automatically
- **Memory Decay**: Less important or rarely accessed memories fade over time
- **Deduplication**: Similar memories are merged to prevent redundancy
- **Content Encryption**: Sensitive data is encrypted at rest using XChaCha20-Poly1305

## How It Works

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│   User Query    │────▶│   RAG Retrieval  │────▶│  Context + Query│
│  "What's my     │     │  (Top 5 similar) │     │  sent to LLM    │
│   favorite..."  │     └──────────────────┘     └─────────────────┘
└─────────────────┘              │                        │
                                 │                        ▼
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│ Stored Memory   │◀────│  Learning Phase  │◀────│   AI Response   │
│ (Encrypted)     │     │ (Q&A + Metadata) │     │                 │
└─────────────────┘     └──────────────────┘     └─────────────────┘
```

### 1. RAG Context Retrieval

When you ask a question:
1. The query is converted to an embedding vector using `nomic-embed-text`
2. Similar memories are found using cosine similarity search
3. The top N most relevant memories are injected into the prompt
4. The AI generates a response with full context

### 2. Automatic Learning

After each response:
1. The Q&A pair is stored as a new memory
2. Embeddings are generated for semantic search
3. If a similar memory exists (>85% similarity), they're merged
4. Content is encrypted before storage

### 3. Memory Types

| Type | Purpose | Example |
|------|---------|---------|
| **Fact** | General knowledge | "Paris is the capital of France" |
| **Preference** | User preferences | "User prefers dark mode" |
| **Correction** | Feedback/corrections | "Actually, the meeting is Tuesday not Monday" |
| **ToolResult** | API/tool outputs | "Weather API returned: 22°C, sunny" |
| **Context** | Conversation context | "Q: What time is it? A: 3:00 PM" |

## Configuration

Add to your `config.toml`:

```toml
[memory]
# Enable memory storage
enabled = true

# Enable RAG context retrieval
enable_rag = true

# Enable automatic learning from interactions
enable_learning = true

# Number of memories to retrieve for RAG context
rag_limit = 5

# Minimum similarity threshold for RAG retrieval (0.0-1.0)
rag_threshold = 0.5

# Similarity threshold for memory deduplication (0.0-1.0)
merge_threshold = 0.85

# Minimum importance score to keep memories
min_importance = 0.1

# Decay factor for memory importance over time
decay_factor = 0.95

# Enable content encryption
enable_encryption = true

# Path to encryption key file (generated if not exists)
encryption_key_path = "memory_encryption.key"

[memory.embedding]
# Embedding model name
model = "nomic-embed-text"

# Embedding dimension
dimension = 384

# Request timeout in milliseconds
timeout_ms = 30000
```

## Memory Decay

Memory importance decays over time based on access patterns:

```
new_importance = importance × decay_factor × (1 + 0.1 × access_boost)
```

Where:
- `decay_factor`: Configurable (default: 0.95)
- `access_boost`: Increases when a memory is retrieved and used

Memories below `min_importance` are automatically cleaned up.

## Security

### Content Encryption

All memory content and summaries are encrypted using:
- **Algorithm**: XChaCha20-Poly1305 (AEAD)
- **Key Size**: 256 bits
- **Nonce Size**: 192 bits (unique per encryption)

The encryption key is stored at `encryption_key_path` and auto-generated if missing.

> ⚠️ **Important**: Backup your encryption key! Without it, encrypted memories cannot be recovered.

### Embedding Vectors

Embedding vectors are stored **unencrypted** to enable similarity search. They reveal:
- Semantic similarity between memories
- General topic clustering

They do NOT reveal:
- Actual content
- Specific details

## Embedding Models

The system supports various Ollama embedding models:

| Model | Dimensions | Use Case |
|-------|------------|----------|
| `nomic-embed-text` | 384 | Default, balanced |
| `mxbai-embed-large` | 1024 | Higher accuracy |
| `bge-m3` | 1024 | Multilingual |

To use a different model:

```toml
[memory.embedding]
model = "mxbai-embed-large"
dimension = 1024
```

## Database Schema

Memories are stored in SQLite with the following structure:

```sql
-- Main memories table
CREATE TABLE memories (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    conversation_id TEXT,
    content TEXT NOT NULL,      -- Encrypted
    summary TEXT NOT NULL,       -- Encrypted
    importance REAL NOT NULL,
    memory_type TEXT NOT NULL,
    tags TEXT NOT NULL,          -- JSON array
    created_at TEXT NOT NULL,
    accessed_at TEXT NOT NULL,
    access_count INTEGER DEFAULT 0
);

-- Embedding vectors for similarity search
CREATE TABLE memory_embeddings (
    memory_id TEXT PRIMARY KEY,
    embedding BLOB NOT NULL      -- Binary float array
);

-- Full-text search index
CREATE VIRTUAL TABLE memory_fts USING fts5(
    id, content, summary, tags
);
```

## Manual Memory Management

You can manually store specific information:

```rust
// Store a fact
memory_service.store_fact(user_id, "User's birthday is March 15", 0.9).await?;

// Store a preference
memory_service.store_preference(user_id, "Prefers metric units", 0.8).await?;

// Store a correction
memory_service.store_correction(user_id, "Actually prefers tea, not coffee", 1.0).await?;
```

## Maintenance

### Applying Decay

Run this periodically (e.g., daily via cron):

```rust
let decayed = memory_service.apply_decay().await?;
println!("Decayed {} memories", decayed.len());
```

### Cleaning Up Low-Importance Memories

```rust
let deleted = memory_service.cleanup_low_importance().await?;
println!("Deleted {} memories", deleted);
```

### Statistics

```rust
let stats = memory_service.stats(&user_id).await?;
println!("Total: {}, With embeddings: {}, Avg importance: {:.2}",
    stats.total_count, stats.with_embeddings, stats.avg_importance);
```

## Troubleshooting

### Memories Not Being Retrieved

1. Check that `enable_rag = true`
2. Verify `rag_threshold` isn't too high (try 0.3)
3. Ensure embeddings are generated (check `with_embeddings` in stats)
4. Confirm Ollama is running with the embedding model

### High Memory Usage

1. Lower `rag_limit` to reduce context size
2. Run `cleanup_low_importance()` more frequently
3. Increase `min_importance` threshold
4. Reduce `decay_factor` for faster decay

### Encryption Key Lost

If you lose the encryption key, encrypted memories **cannot be recovered**.

To start fresh:
1. Delete `memory_encryption.key`
2. Clear the `memories` and `memory_embeddings` tables
3. A new key will be generated on next startup

## API Endpoints

Coming soon: REST API endpoints for memory management.
