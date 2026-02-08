# Configuration Reference

> ⚙️ Complete reference for all PiSovereign configuration options

This document covers every configuration option available in `config.toml`.

## Table of Contents

- [Overview](#overview)
- [Environment Settings](#environment-settings)
- [Server Settings](#server-settings)
- [Inference Engine](#inference-engine)
- [Security Settings](#security-settings)
- [Database & Cache](#database--cache)
  - [Database](#database)
  - [Cache](#cache)
- [Integrations](#integrations)
  - [Messenger Selection](#messenger-selection)
  - [WhatsApp Business](#whatsapp-business)
  - [Signal Messenger](#signal-messenger)
  - [Speech Processing](#speech-processing)
  - [Weather](#weather)
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
# "0.0.0.0" = all interfaces, "127.0.0.1" = localhost only
host = "0.0.0.0"

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
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `host` | String | `0.0.0.0` | Bind address |
| `port` | Integer | `3000` | HTTP port |
| `cors_enabled` | Boolean | `true` | Enable CORS |
| `allowed_origins` | Array | `[]` | CORS allowed origins |
| `shutdown_timeout_secs` | Integer | `30` | Shutdown grace period |
| `log_format` | String | `text` | Log output format |

---

## Inference Engine

```toml
[inference]
# Hailo-Ollama server URL
base_url = "http://localhost:11434"

# Default model for inference
default_model = "qwen2.5-1.5b-instruct"

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
| `default_model` | String | `qwen2.5-1.5b-instruct` | - | Model identifier |
| `timeout_ms` | Integer | `60000` | 1000-300000 | Request timeout |
| `max_tokens` | Integer | `2048` | 1-8192 | Max generation length |
| `temperature` | Float | `0.7` | 0.0-2.0 | Randomness |
| `top_p` | Float | `0.9` | 0.0-1.0 | Nucleus sampling |
| `system_prompt` | String | None | - | Optional system prompt |

---

## Security Settings

```toml
[security]
# Whitelisted phone numbers for WhatsApp
# Empty = allow all, Example: ["+491234567890", "+491234567891"]
whitelisted_phones = []

# API key to User UUID mapping for authentication
# [security.api_key_users]
# "sk-abc123def456" = "550e8400-e29b-41d4-a716-446655440000"
# "sk-xyz789ghi012" = "6ba7b810-9dad-11d1-80b4-00c04fd430c8"

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
| `whitelisted_phones` | Array | `[]` | Allowed phone numbers |
| `api_key_users` | Map | `{}` | API key → User ID mapping |
| `rate_limit_enabled` | Boolean | `true` | Enable rate limiting |
| `rate_limit_rpm` | Integer | `60` | Requests/minute/IP |
| `tls_verify_certs` | Boolean | `true` | Verify TLS certificates |
| `connection_timeout_secs` | Integer | `30` | Connection timeout |
| `min_tls_version` | String | `1.2` | Minimum TLS version |

### API Key Authentication

```toml
[security.api_key_users]
# Format: "api-key" = "user-uuid"
"sk-prod-abc123" = "550e8400-e29b-41d4-a716-446655440000"
```

Usage:
```bash
curl -H "Authorization: Bearer sk-prod-abc123" http://localhost:3000/v1/chat
```

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
```

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
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `phone_number` | String | - | Your Signal phone number (E.164) |
| `socket_path` | String | `/var/run/signal-cli/socket` | signal-cli daemon socket |
| `data_path` | String | - | signal-cli data directory |
| `timeout_ms` | Integer | `30000` | Connection timeout |
| `whitelist` | Array | `[]` | Allowed phone numbers |

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
base_url = "https://api.open-meteo.com/v1"
timeout_secs = 30
forecast_days = 7  # 1-16
cache_ttl_minutes = 30

# Default location (when user has no profile)
default_location = { latitude = 52.52, longitude = 13.405 }  # Berlin
```

### CalDAV Calendar

```toml
[caldav]
# CalDAV server URL (Baïkal, Radicale, Nextcloud)
server_url = "https://cal.example.com"

# Authentication (store in Vault)
username = "your-username"
password = "your-password"

# Default calendar path
calendar_path = "/calendars/user/default"

# TLS verification
verify_certs = true
timeout_secs = 30
```

### Proton Mail

```toml
[proton]
# Proton Bridge connection
imap_host = "127.0.0.1"
imap_port = 1143
smtp_host = "127.0.0.1"
smtp_port = 1025

# Credentials (store in Vault)
email = "user@proton.me"
password = "bridge-password"  # NOT your Proton account password

# TLS settings
[proton.tls]
verify_certificates = false  # Bridge uses self-signed certs
min_tls_version = "1.2"
# ca_cert_path = "/path/to/ca.pem"  # Optional custom CA
```

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

# Cache TTL in minutes (default: 15)
cache_ttl_minutes = 15
```

> **Security Note:** Store the Brave API key in Vault rather than config.toml:
> ```bash
> vault kv put secret/pisovereign/websearch brave_api_key="BSA-..."
> ```

---

## Model Selector

Dynamic model routing based on task complexity:

```toml
[model_selector]
# Model for simple/fast tasks
small_model = "qwen2.5-1.5b-instruct"

# Model for complex/quality tasks
large_model = "qwen2.5-7b-instruct"

# Thresholds for model selection
complexity_word_threshold = 100        # Words in prompt
small_model_max_prompt_chars = 500     # Chars for small model

# Keywords that trigger large model
complexity_keywords = [
    "analyze", "explain", "compare", "summarize",
    "code", "implement", "debug", "refactor",
    "translate", "research"
]
```

---

## Telemetry

```toml
[telemetry]
# Enable OpenTelemetry export
enabled = false

# OTLP endpoint (Tempo, Jaeger)
otlp_endpoint = "http://localhost:4317"

# Sampling ratio (0.0-1.0, 1.0 = all traces)
sample_ratio = 1.0

# Service name for traces
service_name = "pisovereign"

# Log level filter
log_filter = "pisovereign=info,tower_http=info"

# Batch export settings
export_timeout_secs = 30
max_batch_size = 512

# Graceful fallback if collector unavailable
graceful_fallback = true
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

# Cooldown before retrying primary backend
retry_cooldown_secs = 30

# Circuit breaker thresholds
failure_threshold = 3   # Failures before degraded mode
success_threshold = 2   # Successes to exit degraded mode
```

### Retry Configuration

```toml
[retry]
# Exponential backoff settings
initial_delay_ms = 100
max_delay_ms = 10000
multiplier = 2.0
max_retries = 3
```

Formula: `delay = min(initial_delay * multiplier^attempt, max_delay)`

---

## Health Checks

```toml
[health]
# Global timeout for all health checks
global_timeout_secs = 5

# Per-service timeout overrides
# inference_timeout_secs = 10
# email_timeout_secs = 5
# calendar_timeout_secs = 5
# weather_timeout_secs = 5
```

---

## Vault Integration

```toml
[vault]
# Vault server address
address = "http://127.0.0.1:8200"

# AppRole authentication (recommended)
role_id = "your-role-id"
secret_id = "your-secret-id"

# Or token authentication
# token = "hvs.your-token"

# KV engine mount path
mount_path = "secret"

# Request timeout
timeout_secs = 5

# Vault Enterprise namespace (optional)
# namespace = "admin/pisovereign"
```

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
default_model = "qwen2.5-1.5b-instruct"

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
default_model = "qwen2.5-1.5b-instruct"
timeout_ms = 120000

[database]
path = "/var/lib/pisovereign/pisovereign.db"
max_connections = 10

[security]
rate_limit_enabled = true
rate_limit_rpm = 30
min_tls_version = "1.3"

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

[database]
path = "pisovereign.db"
```
