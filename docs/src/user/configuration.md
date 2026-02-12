# Configuration Reference

> ⚙️ Complete reference for all PiSovereign configuration options

This document covers every configuration option available in `config.toml`.

## Table of Contents

- [Overview](#overview)
- [Environment Settings](#environment-settings)
- [Server Settings](#server-settings)
- [Inference Engine](#inference-engine)
- [Security Settings](#security-settings)
  - [Prompt Security](#prompt-security)
  - [API Key Authentication](#api-key-authentication)
- [Memory & Knowledge Storage](#memory--knowledge-storage)
- [Database & Cache](#database--cache)
  - [Database](#database)
  - [Cache](#cache)
- [Integrations](#integrations)
  - [Messenger Selection](#messenger-selection)
  - [WhatsApp Business](#whatsapp-business)
  - [Signal Messenger](#signal-messenger)
  - [Speech Processing](#speech-processing)
  - [Weather](#weather)
  - [Web Search](#web-search)
  - [Public Transit (ÖPNV)](#public-transit-öpnv)
  - [Reminder System](#reminder-system)
  - [CalDAV Calendar](#caldav-calendar)
  - [Proton Mail](#proton-mail)
- [Model Selector](#model-selector)
- [Telemetry](#telemetry)
- [Resilience](#resilience)
  - [Degraded Mode](#degraded-mode)
  - [Retry Configuration](#retry-configuration)
- [Health Checks](#health-checks)
- [Vault Integration](#vault-integration)
- [Environment Variables](#environment-variables)
- [Example Configurations](#example-configurations)

---

## Overview

PiSovereign uses a layered configuration system:

1. **Default values** - Built into the application
2. **Configuration file** - `config.toml` (or path in `PISOVEREIGN_CONFIG`)
3. **Environment variables** - Override config file values

### Configuration File Location

```bash
# Default location
./config.toml

# Custom location
export PISOVEREIGN_CONFIG=/etc/pisovereign/config.toml
```

### Environment Variable Mapping

Config values can be overridden using environment variables:

```
[server]
port = 3000

# Becomes:
PISOVEREIGN_SERVER_PORT=3000
```

Nested values use double underscores:

```
[speech.local_stt]
threads = 4

# Becomes:
PISOVEREIGN_SPEECH_LOCAL_STT__THREADS=4
```

---

## Environment Settings

```toml
# Application environment: "development" or "production"
# In production:
#   - JSON logging is enforced
#   - Security warnings block startup (unless PISOVEREIGN_ALLOW_INSECURE_CONFIG=true)
#   - TLS verification is enforced
environment = "development"
```

| Value | Description |
|-------|-------------|
| `development` | Relaxed security, human-readable logs |
| `production` | Strict security, JSON logs, TLS enforced |

---

## Server Settings

```toml
[server]
# Network interface to bind to
# "127.0.0.1" = localhost only (recommended for security)
# "0.0.0.0" = all interfaces (use behind reverse proxy)
host = "127.0.0.1"

# HTTP port
port = 3000

# Enable CORS (Cross-Origin Resource Sharing)
cors_enabled = true

# Allowed CORS origins
# Empty array = allow all (WARNING in production)
# Example: ["https://app.example.com", "https://admin.example.com"]
allowed_origins = []

# Graceful shutdown timeout (seconds)
# Time to wait for active requests to complete
shutdown_timeout_secs = 30

# Log format: "json" or "text"
# In production mode, defaults to "json" even if set to "text"
log_format = "text"

# Maximum request body size for JSON payloads (optional, bytes)
# max_body_size_json_bytes = 1048576  # 1MB

# Maximum request body size for audio uploads (optional, bytes)
# max_body_size_audio_bytes = 10485760  # 10MB
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `host` | String | `127.0.0.1` | Bind address |
| `port` | Integer | `3000` | HTTP port |
| `cors_enabled` | Boolean | `true` | Enable CORS |
| `allowed_origins` | Array | `[]` | CORS allowed origins |
| `shutdown_timeout_secs` | Integer | `30` | Shutdown grace period |
| `log_format` | String | `text` | Log output format |
| `max_body_size_json_bytes` | Integer | `1048576` | **(Optional)** Max JSON payload size |
| `max_body_size_audio_bytes` | Integer | `10485760` | **(Optional)** Max audio upload size |

---

## Inference Engine

```toml
[inference]
# Ollama-compatible server URL
# Works with both hailo-ollama (Raspberry Pi) and standard Ollama (macOS)
base_url = "http://localhost:11434"

# Default model for inference
default_model = "qwen2.5:1.5b"

# Request timeout (milliseconds)
timeout_ms = 60000

# Maximum tokens to generate
max_tokens = 2048

# Sampling temperature (0.0 = deterministic, 2.0 = creative)
temperature = 0.7

# Top-p (nucleus) sampling (0.0-1.0)
top_p = 0.9

# System prompt (optional)
# system_prompt = "You are a helpful AI assistant."
```

| Option | Type | Default | Range | Description |
|--------|------|---------|-------|-------------|
| `base_url` | String | `http://localhost:11434` | - | Inference server URL |
| `default_model` | String | `qwen2.5:1.5b` | - | Model identifier |
| `timeout_ms` | Integer | `60000` | 1000-300000 | Request timeout |
| `max_tokens` | Integer | `2048` | 1-8192 | Max generation length |
| `temperature` | Float | `0.7` | 0.0-2.0 | Randomness |
| `top_p` | Float | `0.9` | 0.0-1.0 | Nucleus sampling |
| `system_prompt` | String | None | - | **(Optional)** System prompt |

---

## Security Settings

```toml
[security]
# Whitelisted phone numbers for WhatsApp
# Empty = allow all, Example: ["+491234567890", "+491234567891"]
whitelisted_phones = []

# API Keys (hashed with Argon2id)
# Generate hashed keys using: pisovereign-cli hash-api-key <your-key>
# Migrate existing plaintext keys: pisovereign-cli migrate-keys --input config.toml --dry-run
#
# [[security.api_keys]]
# hash = "$argon2id$v=19$m=19456,t=2,p=1$..."
# user_id = "550e8400-e29b-41d4-a716-446655440000"
#
# [[security.api_keys]]
# hash = "$argon2id$v=19$m=19456,t=2,p=1$..."
# user_id = "6ba7b810-9dad-11d1-80b4-00c04fd430c8"

# Trusted reverse proxies (IP addresses) - optional
# Add your proxy IPs here if behind a reverse proxy
# trusted_proxies = ["127.0.0.1", "::1"]

# Rate limiting
rate_limit_enabled = true
rate_limit_rpm = 60  # Requests per minute per IP

# TLS settings for outbound connections
tls_verify_certs = true
connection_timeout_secs = 30
min_tls_version = "1.2"  # "1.2" or "1.3"
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `whitelisted_phones` | Array | `[]` | **(Optional)** Allowed phone numbers |
| `api_keys` | Array | `[]` | API key definitions with Argon2id hash |
| `trusted_proxies` | Array | - | **(Optional)** Trusted reverse proxy IPs |
| `rate_limit_enabled` | Boolean | `true` | Enable rate limiting |
| `rate_limit_rpm` | Integer | `60` | Requests/minute/IP |
| `tls_verify_certs` | Boolean | `true` | Verify TLS certificates for outbound connections |
| `connection_timeout_secs` | Integer | `30` | Connection timeout for external services |
| `min_tls_version` | String | `1.2` | Minimum TLS version ("1.2" or "1.3") |

### Prompt Security

Protects against prompt injection and other AI security threats.

```toml
[prompt_security]
# Enable prompt security analysis
enabled = true

# Sensitivity level: "low", "medium", or "high"
# - low: Only block high-confidence threats
# - medium: Block medium and high confidence threats (recommended)
# - high: Block all detected threats including low confidence
sensitivity = "medium"

# Block requests when security threats are detected
block_on_detection = true

# Maximum violations before auto-blocking an IP
max_violations_before_block = 3

# Time window for counting violations (seconds)
violation_window_secs = 3600  # 1 hour

# How long to block an IP after exceeding max violations (seconds)
block_duration_secs = 86400  # 24 hours

# Immediately block IPs that send critical-level threats
auto_block_on_critical = true

# Custom patterns to detect (in addition to built-in patterns) - optional
# custom_patterns = ["DROP TABLE", "eval("]
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enabled` | Boolean | `true` | Enable prompt security analysis |
| `sensitivity` | String | `medium` | Detection level: "low", "medium", or "high" |
| `block_on_detection` | Boolean | `true` | Block requests when threats detected |
| `max_violations_before_block` | Integer | `3` | Violations before IP auto-block |
| `violation_window_secs` | Integer | `3600` | Time window for counting violations |
| `block_duration_secs` | Integer | `86400` | IP block duration after violations |
| `auto_block_on_critical` | Boolean | `true` | Auto-block critical threats immediately |
| `custom_patterns` | Array | - | **(Optional)** Custom threat detection patterns |

### API Key Authentication

API keys are now securely hashed using Argon2id. Use the CLI tools to generate and migrate keys.

**Generate a new hashed key:**
```bash
pisovereign-cli hash-api-key <your-api-key>
```

**Migrate existing plaintext keys:**
```bash
pisovereign-cli migrate-keys --input config.toml --dry-run
pisovereign-cli migrate-keys --input config.toml --output config-new.toml
```

**Configuration:**
```toml
[[security.api_keys]]
hash = "$argon2id$v=19$m=19456,t=2,p=1$..."
user_id = "550e8400-e29b-41d4-a716-446655440000"
```

**Usage:**
```bash
curl -H "Authorization: Bearer <your-api-key>" http://localhost:3000/v1/chat
```

---

## Memory & Knowledge Storage

Persistent AI memory for RAG-based context retrieval. Stores interactions, facts, preferences, and corrections using embeddings for semantic similarity search.

```toml
[memory]
# Enable memory storage (default: true)
# enabled = true

# Enable RAG context retrieval (default: true)
# enable_rag = true

# Enable automatic learning from interactions (default: true)
# enable_learning = true

# Number of memories to retrieve for RAG context (default: 5)
# rag_limit = 5

# Minimum similarity threshold for RAG retrieval (0.0-1.0, default: 0.5)
# rag_threshold = 0.5

# Similarity threshold for memory deduplication (0.0-1.0, default: 0.85)
# merge_threshold = 0.85

# Minimum importance score to keep memories (default: 0.1)
# min_importance = 0.1

# Decay factor for memory importance over time (default: 0.95)
# decay_factor = 0.95

# Enable content encryption (default: true)
# enable_encryption = true

# Path to encryption key file (generated if not exists)
# encryption_key_path = "memory_encryption.key"

[memory.embedding]
# Embedding model name (default: nomic-embed-text)
# model = "nomic-embed-text"

# Embedding dimension (default: 384 for nomic-embed-text)
# dimension = 384

# Request timeout in milliseconds (default: 30000)
# timeout_ms = 30000
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enabled` | Boolean | `true` | **(Optional)** Enable memory storage |
| `enable_rag` | Boolean | `true` | **(Optional)** Enable RAG context retrieval |
| `enable_learning` | Boolean | `true` | **(Optional)** Auto-learn from interactions |
| `rag_limit` | Integer | `5` | **(Optional)** Number of memories for RAG |
| `rag_threshold` | Float | `0.5` | **(Optional)** Min similarity for RAG (0.0-1.0) |
| `merge_threshold` | Float | `0.85` | **(Optional)** Similarity for deduplication (0.0-1.0) |
| `min_importance` | Float | `0.1` | **(Optional)** Min importance to keep memories |
| `decay_factor` | Float | `0.95` | **(Optional)** Importance decay over time |
| `enable_encryption` | Boolean | `true` | **(Optional)** Encrypt stored content |
| `encryption_key_path` | String | `memory_encryption.key` | **(Optional)** Encryption key file path |

**Embedding Settings:**

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `embedding.model` | String | `nomic-embed-text` | **(Optional)** Embedding model name |
| `embedding.dimension` | Integer | `384` | **(Optional)** Embedding vector dimension |
| `embedding.timeout_ms` | Integer | `30000` | **(Optional)** Request timeout |

---

## Database & Cache

### Database

```toml
[database]
# SQLite database file path
path = "pisovereign.db"

# Connection pool size
max_connections = 5

# Auto-run migrations on startup
run_migrations = true
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `path` | String | `pisovereign.db` | Database file path |
| `max_connections` | Integer | `5` | Pool size |
| `run_migrations` | Boolean | `true` | Auto-migrate |

### Cache

```toml
[cache]
# Enable caching (disable for debugging)
enabled = true

# TTL values (seconds)
ttl_short_secs = 300       # 5 minutes - frequently changing
ttl_medium_secs = 3600     # 1 hour - moderately stable
ttl_long_secs = 86400      # 24 hours - stable data

# LLM response caching
ttl_llm_dynamic_secs = 3600   # Dynamic content (briefings)
ttl_llm_stable_secs = 86400   # Stable content (help text)

# L1 (in-memory) cache size
l1_max_entries = 10000
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enabled` | Boolean | `true` | Enable caching |
| `ttl_short_secs` | Integer | `300` | Short TTL |
| `ttl_medium_secs` | Integer | `3600` | Medium TTL |
| `ttl_long_secs` | Integer | `86400` | Long TTL |
| `ttl_llm_dynamic_secs` | Integer | `3600` | Dynamic LLM TTL |
| `ttl_llm_stable_secs` | Integer | `86400` | Stable LLM TTL |
| `l1_max_entries` | Integer | `10000` | Max memory cache entries |

---

## Integrations

### Messenger Selection

PiSovereign supports one messenger at a time:

```toml
# Choose one: "whatsapp", "signal", or "none"
messenger = "whatsapp"
```

| Value | Description |
|-------|-------------|
| `whatsapp` | Use WhatsApp Business API (webhooks) |
| `signal` | Use Signal via signal-cli (polling) |
| `none` | Disable messenger integration |

### WhatsApp Business

```toml
[whatsapp]
# Meta Graph API access token (store in Vault)
# access_token = "your-access-token"

# Phone number ID from WhatsApp Business
# phone_number_id = "your-phone-number-id"

# App secret for webhook signature verification
# app_secret = "your-app-secret"

# Verify token for webhook setup
# verify_token = "your-verify-token"

# Require webhook signature verification
signature_required = true

# Meta Graph API version
api_version = "v18.0"

# Phone numbers allowed to send messages (empty = allow all)
# whitelist = ["+1234567890"]

# Conversation Persistence Settings
[whatsapp.persistence]
# Enable conversation persistence (default: true)
# enabled = true

# Enable encryption for stored messages (default: true)
# enable_encryption = true

# Enable RAG context retrieval from memory system (default: true)
# enable_rag = true

# Enable automatic learning from interactions (default: true)
# enable_learning = true

# Maximum days to retain conversations (optional, unlimited if not set)
# retention_days = 90

# Maximum messages per conversation before FIFO truncation (optional)
# max_messages_per_conversation = 1000

# Number of recent messages to use as context (default: 50)
# context_window = 50
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `access_token` | String | - | **(Optional)** Meta Graph API token (store in Vault) |
| `phone_number_id` | String | - | **(Optional)** WhatsApp Business phone number ID |
| `app_secret` | String | - | **(Optional)** Webhook signature secret |
| `verify_token` | String | - | **(Optional)** Webhook verification token |
| `signature_required` | Boolean | `true` | Require webhook signature verification |
| `api_version` | String | `v18.0` | Meta Graph API version |
| `whitelist` | Array | `[]` | **(Optional)** Allowed phone numbers |

**Persistence Options:**

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `persistence.enabled` | Boolean | `true` | **(Optional)** Store conversations in database |
| `persistence.enable_encryption` | Boolean | `true` | **(Optional)** Encrypt stored messages |
| `persistence.enable_rag` | Boolean | `true` | **(Optional)** Enable RAG context retrieval |
| `persistence.enable_learning` | Boolean | `true` | **(Optional)** Auto-learn from interactions |
| `persistence.retention_days` | Integer | - | **(Optional)** Max retention days (unlimited if not set) |
| `persistence.max_messages_per_conversation` | Integer | - | **(Optional)** Max messages before truncation |
| `persistence.context_window` | Integer | `50` | **(Optional)** Recent messages for context |

### Signal Messenger

```toml
[signal]
# Your phone number registered with Signal (E.164 format)
phone_number = "+1234567890"

# Path to signal-cli JSON-RPC socket
socket_path = "/var/run/signal-cli/socket"

# Path to signal-cli data directory (optional)
# data_path = "/var/lib/signal-cli"

# Connection timeout in milliseconds
timeout_ms = 30000

# Phone numbers allowed to send messages (empty = allow all)
# whitelist = ["+1234567890", "+0987654321"]

# Conversation Persistence Settings
[signal.persistence]
# Enable conversation persistence (default: true)
# enabled = true

# Enable encryption for stored messages (default: true)
# enable_encryption = true

# Enable RAG context retrieval from memory system (default: true)
# enable_rag = true

# Enable automatic learning from interactions (default: true)
# enable_learning = true

# Maximum days to retain conversations (optional, unlimited if not set)
# retention_days = 90

# Maximum messages per conversation before FIFO truncation (optional)
# max_messages_per_conversation = 1000

# Number of recent messages to use as context (default: 50)
# context_window = 50
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `phone_number` | String | - | Your Signal phone number (E.164) |
| `socket_path` | String | `/var/run/signal-cli/socket` | signal-cli daemon socket |
| `data_path` | String | - | **(Optional)** signal-cli data directory |
| `timeout_ms` | Integer | `30000` | Connection timeout |
| `whitelist` | Array | `[]` | **(Optional)** Allowed phone numbers |

**Persistence Options:**

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `persistence.enabled` | Boolean | `true` | **(Optional)** Store conversations in database |
| `persistence.enable_encryption` | Boolean | `true` | **(Optional)** Encrypt stored messages |
| `persistence.enable_rag` | Boolean | `true` | **(Optional)** Enable RAG context retrieval |
| `persistence.enable_learning` | Boolean | `true` | **(Optional)** Auto-learn from interactions |
| `persistence.retention_days` | Integer | - | **(Optional)** Max retention days (unlimited if not set) |
| `persistence.max_messages_per_conversation` | Integer | - | **(Optional)** Max messages before truncation |
| `persistence.context_window` | Integer | `50` | **(Optional)** Recent messages for context |

📖 See [Signal Setup Guide](./signal-setup.md) for installation instructions.

### Speech Processing

```toml
[speech]
# Provider: "hybrid" (default), "local", or "openai"
provider = "hybrid"

# OpenAI settings (for cloud/hybrid)
openai_api_key = "sk-..."
openai_base_url = "https://api.openai.com/v1"
stt_model = "whisper-1"
tts_model = "tts-1"
default_voice = "nova"  # alloy, echo, fable, onyx, nova, shimmer
output_format = "opus"  # opus, ogg, mp3, wav
timeout_ms = 60000
max_audio_duration_ms = 1500000  # 25 minutes
response_format = "mirror"  # mirror, text, voice
speed = 1.0  # 0.25 to 4.0

# Local STT (whisper.cpp)
[speech.local_stt]
executable_path = "whisper-cpp"
model_path = "/usr/local/share/whisper/ggml-base.bin"
threads = 4
default_language = "en"

# Local TTS (Piper)
[speech.local_tts]
executable_path = "piper"
default_model_path = "/usr/local/share/piper/voices/en_US-lessac-medium.onnx"
default_voice = "en_US-lessac-medium"
length_scale = 1.0      # Speaking rate
sentence_silence = 0.2  # Pause between sentences

# Hybrid mode settings
[speech.hybrid]
prefer_local = true          # Try local first
allow_cloud_fallback = true  # Fall back to OpenAI if local fails
```

### Weather

```toml
[weather]
# Open-Meteo API (free, no key required)
# base_url = "https://api.open-meteo.com/v1"

# Connection timeout in seconds
# timeout_secs = 30

# Number of forecast days (1-16)
# forecast_days = 7

# Cache TTL in minutes
# cache_ttl_minutes = 30

# Default location (when user has no profile)
# default_location = { latitude = 52.52, longitude = 13.405 }  # Berlin
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `base_url` | String | `https://api.open-meteo.com/v1` | **(Optional)** Open-Meteo API URL |
| `timeout_secs` | Integer | `30` | **(Optional)** Request timeout |
| `forecast_days` | Integer | `7` | **(Optional)** Forecast days (1-16) |
| `cache_ttl_minutes` | Integer | `30` | **(Optional)** Cache TTL |
| `default_location` | Object | - | **(Optional)** Default location `{ latitude, longitude }` |

### CalDAV Calendar

```toml
[caldav]
# CalDAV server URL (Baïkal, Radicale, Nextcloud)
# server_url = "https://cal.example.com"
# When using Baïkal via Docker (setup --baikal):
# server_url = "http://baikal:80/dav.php"

# Authentication (store in Vault)
# username = "your-username"
# password = "your-password"

# Default calendar path (optional)
# calendar_path = "/calendars/user/default"

# TLS verification
# verify_certs = true

# Connection timeout in seconds
# timeout_secs = 30
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `server_url` | String | - | **(Optional)** CalDAV server URL |
| `username` | String | - | **(Optional)** Username for authentication (store in Vault) |
| `password` | String | - | **(Optional)** Password for authentication (store in Vault) |
| `calendar_path` | String | `/calendars/user/default` | **(Optional)** Default calendar path |
| `verify_certs` | Boolean | `true` | **(Optional)** Verify TLS certificates |
| `timeout_secs` | Integer | `30` | **(Optional)** Connection timeout |

### Proton Mail

```toml
[proton]
# IMAP server host (Proton Bridge)
# imap_host = "127.0.0.1"

# IMAP server port (default: 1143 for STARTTLS)
# imap_port = 1143

# SMTP server host (Proton Bridge)
# smtp_host = "127.0.0.1"

# SMTP server port (default: 1025 for STARTTLS)
# smtp_port = 1025

# Email address (Bridge account email)
# email = "user@proton.me"

# Bridge password (from Bridge UI, NOT Proton account password)
# password = "bridge-password"

# TLS configuration
[proton.tls]
# Verify TLS certificates (omit to verify, set false for self-signed Bridge certs)
# verify_certificates = false

# Minimum TLS version
# min_tls_version = "1.2"

# Custom CA certificate path (optional)
# ca_cert_path = "/path/to/ca.pem"
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `imap_host` | String | `127.0.0.1` | **(Optional)** IMAP server host |
| `imap_port` | Integer | `1143` | **(Optional)** IMAP server port |
| `smtp_host` | String | `127.0.0.1` | **(Optional)** SMTP server host |
| `smtp_port` | Integer | `1025` | **(Optional)** SMTP server port |
| `email` | String | - | **(Optional)** Email address (store in Vault) |
| `password` | String | - | **(Optional)** Bridge password (store in Vault) |
| `tls.verify_certificates` | Boolean | `true` | **(Optional)** Verify TLS certificates |
| `tls.min_tls_version` | String | `1.2` | **(Optional)** Minimum TLS version |
| `tls.ca_cert_path` | String | - | **(Optional)** Custom CA certificate path |

### Web Search

```toml
[websearch]
# Brave Search API key (required for primary provider)
# Get your key at: https://brave.com/search/api/
# api_key = "BSA-your-brave-api-key"

# Maximum results per search query (default: 5)
max_results = 5

# Request timeout in seconds (default: 30)
timeout_secs = 30

# Enable DuckDuckGo fallback if Brave fails (default: true)
fallback_enabled = true

# Safe search: "off", "moderate", "strict" (default: "moderate")
safe_search = "moderate"

# Country code for localized results (e.g., "US", "DE", "GB")
country = "DE"

# Language code for results (e.g., "en", "de", "fr")
language = "de"

# Rate limit: requests per minute (default: 60)
rate_limit_rpm = 60

# Cache TTL in minutes (default: 30)
cache_ttl_minutes = 30
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `api_key` | String | - | **(Optional)** Brave Search API key (store in Vault) |
| `max_results` | Integer | `5` | **(Optional)** Max search results (1-10) |
| `timeout_secs` | Integer | `30` | **(Optional)** Request timeout |
| `fallback_enabled` | Boolean | `true` | **(Optional)** Enable DuckDuckGo fallback |
| `safe_search` | String | `moderate` | **(Optional)** Safe search: "off", "moderate", "strict" |
| `country` | String | `DE` | **(Optional)** Country code for results |
| `language` | String | `de` | **(Optional)** Language code for results |
| `rate_limit_rpm` | Integer | `60` | **(Optional)** Rate limit (requests/minute) |
| `cache_ttl_minutes` | Integer | `30` | **(Optional)** Cache time-to-live |

> **Security Note:** Store the Brave API key in Vault rather than config.toml:
> ```bash
> vault kv put secret/pisovereign/websearch brave_api_key="BSA-..."
> ```

### Public Transit (ÖPNV)

Provides public transit routing for German transport networks via transport.rest API. Used for "How do I get to X?" queries and location-based reminders.

```toml
[transit]
# Base URL for transport.rest API (default: v6.db.transport.rest)
# base_url = "https://v6.db.transport.rest"

# Request timeout in seconds
# timeout_secs = 10

# Maximum number of journey results
# max_results = 3

# Cache TTL in minutes
# cache_ttl_minutes = 5

# Include transit info in location-based reminders
# include_in_reminders = true

# Transport modes to include:
# products_bus = true
# products_suburban = true  # S-Bahn
# products_subway = true    # U-Bahn
# products_tram = true
# products_regional = true  # RB/RE
# products_national = false # ICE/IC

# User's home location for route calculations
# home_location = { latitude = 52.52, longitude = 13.405 }  # Berlin
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `base_url` | String | `https://v6.db.transport.rest` | **(Optional)** transport.rest API URL |
| `timeout_secs` | Integer | `10` | **(Optional)** Request timeout |
| `max_results` | Integer | `3` | **(Optional)** Max journey results |
| `cache_ttl_minutes` | Integer | `5` | **(Optional)** Cache TTL |
| `include_in_reminders` | Boolean | `true` | **(Optional)** Include in location reminders |
| `products_bus` | Boolean | `true` | **(Optional)** Include bus routes |
| `products_suburban` | Boolean | `true` | **(Optional)** Include S-Bahn |
| `products_subway` | Boolean | `true` | **(Optional)** Include U-Bahn |
| `products_tram` | Boolean | `true` | **(Optional)** Include tram |
| `products_regional` | Boolean | `true` | **(Optional)** Include regional trains (RB/RE) |
| `products_national` | Boolean | `false` | **(Optional)** Include national trains (ICE/IC) |
| `home_location` | Object | - | **(Optional)** Home location `{ latitude, longitude }` |

### Reminder System

Configures the proactive reminder system including CalDAV sync, custom reminders, and scheduling settings.

```toml
[reminder]
# Maximum number of snoozes per reminder
# max_snooze = 5

# Default snooze duration in minutes
# default_snooze_minutes = 15

# How far in advance to create reminders from CalDAV events (minutes)
# caldav_reminder_lead_time_minutes = 30

# Interval for checking due reminders (seconds)
# check_interval_secs = 60

# CalDAV sync interval (minutes)
# caldav_sync_interval_minutes = 15

# Morning briefing time (HH:MM format)
# morning_briefing_time = "07:00"

# Enable morning briefing
# morning_briefing_enabled = true
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `max_snooze` | Integer | `5` | **(Optional)** Max snoozes per reminder |
| `default_snooze_minutes` | Integer | `15` | **(Optional)** Default snooze duration |
| `caldav_reminder_lead_time_minutes` | Integer | `30` | **(Optional)** CalDAV event advance notice |
| `check_interval_secs` | Integer | `60` | **(Optional)** How often to check for due reminders |
| `caldav_sync_interval_minutes` | Integer | `15` | **(Optional)** CalDAV sync frequency |
| `morning_briefing_time` | String | `07:00` | **(Optional)** Morning briefing time (HH:MM) |
| `morning_briefing_enabled` | Boolean | `true` | **(Optional)** Enable daily morning briefing |

---

## Model Selector

Dynamic model routing based on task complexity:

```toml
[model_selector]
# Model for simple/fast tasks
# small_model = "qwen2.5-1.5b-instruct"

# Model for complex/quality tasks
# large_model = "qwen2.5-7b-instruct"

# Word count threshold to trigger large model
# complexity_word_threshold = 100

# Maximum prompt length (chars) for small model
# small_model_max_prompt_chars = 500

# Keywords that trigger large model usage
# complexity_keywords = [
#     "analyze", "explain", "compare", "summarize",
#     "code", "implement", "debug", "refactor",
#     "translate", "research"
# ]
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `small_model` | String | `qwen2.5-1.5b-instruct` | **(Optional)** Model for simple/fast tasks |
| `large_model` | String | `qwen2.5-7b-instruct` | **(Optional)** Model for complex/quality tasks |
| `complexity_word_threshold` | Integer | `100` | **(Optional)** Word count to trigger large model |
| `small_model_max_prompt_chars` | Integer | `500` | **(Optional)** Max prompt chars for small model |
| `complexity_keywords` | Array | See above | **(Optional)** Keywords that trigger large model |

---

## Telemetry

```toml
[telemetry]
# Enable OpenTelemetry export
enabled = false

# OTLP endpoint (Tempo, Jaeger)
# otlp_endpoint = "http://localhost:4317"

# Sampling ratio (0.0-1.0, 1.0 = all traces)
# sample_ratio = 1.0

# Service name for traces
# service_name = "pisovereign"

# Log level filter (e.g., "info", "debug", "pisovereign=debug,tower_http=info")
# log_filter = "pisovereign=info,tower_http=info"

# Batch export timeout in seconds
# export_timeout_secs = 30

# Maximum batch size for trace export
# max_batch_size = 512

# Graceful fallback to console-only logging if OTLP collector is unavailable.
# When true (default), the application starts with console logging if the collector
# cannot be reached. Set to false to require a working collector in production.
# graceful_fallback = true
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enabled` | Boolean | `false` | Enable OpenTelemetry export |
| `otlp_endpoint` | String | `http://localhost:4317` | **(Optional)** OTLP collector endpoint |
| `sample_ratio` | Float | `1.0` | **(Optional)** Trace sampling ratio (0.0-1.0) |
| `service_name` | String | `pisovereign` | **(Optional)** Service name for traces |
| `log_filter` | String | `pisovereign=info,tower_http=info` | **(Optional)** Log level filter |
| `export_timeout_secs` | Integer | `30` | **(Optional)** Batch export timeout |
| `max_batch_size` | Integer | `512` | **(Optional)** Max batch size for export |
| `graceful_fallback` | Boolean | `true` | **(Optional)** Fallback to console logging if collector unavailable |
```

---

## Resilience

### Degraded Mode

```toml
[degraded_mode]
# Enable fallback when backend unavailable
enabled = true

# Message returned during degraded mode
unavailable_message = "I'm currently experiencing technical difficulties. Please try again in a moment."

# Cooldown before retrying primary backend (seconds)
retry_cooldown_secs = 30

# Number of failures before entering degraded mode
failure_threshold = 3

# Number of successes required to exit degraded mode
success_threshold = 2
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enabled` | Boolean | `true` | Enable degraded mode fallback |
| `unavailable_message` | String | See above | Message returned during degraded mode |
| `retry_cooldown_secs` | Integer | `30` | Cooldown before retrying primary backend |
| `failure_threshold` | Integer | `3` | Failures before entering degraded mode |
| `success_threshold` | Integer | `2` | Successes to exit degraded mode |

### Retry Configuration

Exponential backoff for retrying failed requests.

```toml
[retry]
# Initial delay before first retry in milliseconds
initial_delay_ms = 100

# Maximum delay between retries in milliseconds
max_delay_ms = 10000

# Multiplier for exponential backoff (delay = initial * multiplier^attempt)
multiplier = 2.0

# Maximum number of retry attempts
max_retries = 3
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `initial_delay_ms` | Integer | `100` | Initial retry delay (milliseconds) |
| `max_delay_ms` | Integer | `10000` | Maximum retry delay (milliseconds) |
| `multiplier` | Float | `2.0` | Exponential backoff multiplier |
| `max_retries` | Integer | `3` | Maximum retry attempts |

**Formula:** `delay = min(initial_delay * multiplier^attempt, max_delay)`

---

## Health Checks

```toml
[health]
# Global timeout for all health checks in seconds
global_timeout_secs = 5

# Service-specific timeout overrides (uncomment to customize):
# inference_timeout_secs = 10
# email_timeout_secs = 5
# calendar_timeout_secs = 5
# weather_timeout_secs = 5
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `global_timeout_secs` | Integer | `5` | Global timeout for all health checks |
| `inference_timeout_secs` | Integer | `5` | **(Optional)** Inference service timeout override |
| `email_timeout_secs` | Integer | `5` | **(Optional)** Email service timeout override |
| `calendar_timeout_secs` | Integer | `5` | **(Optional)** Calendar service timeout override |
| `weather_timeout_secs` | Integer | `5` | **(Optional)** Weather service timeout override |

---

## Vault Integration

```toml
[vault]
# Vault server address
# address = "http://127.0.0.1:8200"

# AppRole authentication (recommended)
# role_id = "your-role-id"
# secret_id = "your-secret-id"

# Or token authentication
# token = "hvs.your-token"

# KV engine mount path
# mount_path = "secret"

# Request timeout in seconds
# timeout_secs = 5

# Vault Enterprise namespace (optional)
# namespace = "admin/pisovereign"
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `address` | String | `http://127.0.0.1:8200` | **(Optional)** Vault server address |
| `role_id` | String | - | **(Optional)** AppRole role ID (recommended) |
| `secret_id` | String | - | **(Optional)** AppRole secret ID |
| `token` | String | - | **(Optional)** Vault token (alternative to AppRole) |
| `mount_path` | String | `secret` | **(Optional)** KV engine mount path |
| `timeout_secs` | Integer | `5` | **(Optional)** Request timeout |
| `namespace` | String | - | **(Optional)** Vault Enterprise namespace |

---

## Environment Variables

All configuration options can be set via environment variables:

| Config Path | Environment Variable |
|-------------|---------------------|
| `server.port` | `PISOVEREIGN_SERVER_PORT` |
| `inference.base_url` | `PISOVEREIGN_INFERENCE_BASE_URL` |
| `security.rate_limit_rpm` | `PISOVEREIGN_SECURITY_RATE_LIMIT_RPM` |
| `database.path` | `PISOVEREIGN_DATABASE_PATH` |
| `vault.address` | `PISOVEREIGN_VAULT_ADDRESS` |

Special variables:

| Variable | Description |
|----------|-------------|
| `PISOVEREIGN_CONFIG` | Config file path |
| `PISOVEREIGN_ALLOW_INSECURE_CONFIG` | Allow insecure settings in production |
| `RUST_LOG` | Log level override |

---

## Example Configurations

### Development

```toml
environment = "development"

[server]
host = "127.0.0.1"
port = 3000
log_format = "text"

[inference]
base_url = "http://localhost:11434"
default_model = "qwen2.5:1.5b"

[database]
path = "./dev.db"

[cache]
enabled = false  # Disable for debugging

[security]
rate_limit_enabled = false
tls_verify_certs = false
```

### Production

```toml
environment = "production"

[server]
host = "127.0.0.1"  # Behind reverse proxy
port = 3000
log_format = "json"
cors_enabled = true
allowed_origins = ["https://app.example.com"]

[inference]
base_url = "http://localhost:11434"
default_model = "qwen2.5:1.5b"
timeout_ms = 120000

[database]
path = "/var/lib/pisovereign/pisovereign.db"
max_connections = 10

[security]
rate_limit_enabled = true
rate_limit_rpm = 30
min_tls_version = "1.3"

[prompt_security]
enabled = true
sensitivity = "high"
block_on_detection = true

[vault]
address = "https://vault.internal:8200"
role_id = "..."
mount_path = "secret"

[telemetry]
enabled = true
otlp_endpoint = "http://tempo:4317"
sample_ratio = 0.1
```

### Minimal (Quick Start)

```toml
environment = "development"

[server]
port = 3000

[inference]
base_url = "http://localhost:11434"
default_model = "qwen2.5:1.5b"

[database]
path = "pisovereign.db"
```
