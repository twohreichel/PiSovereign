# PiSovereign

[![CI](https://github.com/twohreichel/PiSovereign/actions/workflows/ci.yml/badge.svg)](https://github.com/twohreichel/PiSovereign/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/twohreichel/PiSovereign/graph/badge.svg)](https://codecov.io/gh/twohreichel/PiSovereign)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

🤖 Local, secure AI assistant platform for Raspberry Pi 5 + Hailo-10H AI HAT+ 2.

## Features

- **Local LLM Inference** on Hailo-10H (Qwen2.5-1.5B, Llama3.2-1B)
- **WhatsApp Control** – Send commands via message
- **Voice Messages** – STT/TTS for voice-based interaction
- **Calendar Integration** (CalDAV: Baïkal, Radicale)
- **Email Integration** (Proton Mail Bridge)
- **EU/GDPR Compliant** – Everything local, European services

## Quick Start

### Prerequisites

- Raspberry Pi 5 (8 GB RAM)
- Hailo AI HAT+ 2 (Hailo-10H)
- Raspberry Pi OS Trixie (64-bit)
- Rust 1.85+ (Edition 2024)

### Installation

```bash
# 1. Clone repository
git clone https://github.com/andreasreichel/PiSovereign.git
cd PiSovereign

# 2. Install Hailo packages (on Pi)
sudo apt install hailo-h10-all

# 3. Start Hailo-Ollama
hailo-ollama &

# 4. Build PiSovereign
cargo build --release

# 5. Start server
./target/release/pisovereign-server
```

### CLI Usage

```bash
# Query status
pisovereign-cli status

# Send chat message
pisovereign-cli chat "What's the weather tomorrow?"

# Execute command
pisovereign-cli command "briefing"
```

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Liveness check |
| `/ready` | GET | Readiness check with Hailo status |
| `/v1/chat` | POST | Send chat message |
| `/v1/chat/stream` | POST | Streaming chat (SSE) |
| `/v1/commands` | POST | Execute command |
| `/v1/commands/parse` | POST | Parse command without execution |
| `/v1/system/status` | GET | System status |
| `/v1/system/models` | GET | Available models |

## Project Structure

```
crates/
├── domain/              # Core entities, value objects, commands
├── application/         # Use cases, services, ports
├── infrastructure/      # Adapters (Hailo, DB, etc.)
├── ai_core/            # Inference engine, Hailo client
├── ai_speech/          # Speech-to-Text/Text-to-Speech (OpenAI Whisper/TTS)
├── presentation_http/   # HTTP-API (Axum)
├── presentation_cli/    # CLI tool
├── integration_whatsapp/# WhatsApp Business API
├── integration_caldav/  # CalDAV client
└── integration_proton/  # Proton Mail Bridge
```

## Configuration

Environment variables or `config.toml`:

```bash
export PISOVEREIGN_SERVER_PORT=3000
export PISOVEREIGN_INFERENCE_BASE_URL=http://localhost:11434
export PISOVEREIGN_INFERENCE_DEFAULT_MODEL=qwen2.5-1.5b-instruct
```

## Production Deployment

### TLS with Traefik (Recommended)

PiSovereign includes a production-ready Docker Compose configuration with automatic TLS via Let's Encrypt:

```bash
# Copy production config
cp docker-compose.production.yml docker-compose.override.yml

# Set your domain
export DOMAIN=pisovereign.example.com
export ACME_EMAIL=admin@example.com

# Start with production profile
docker compose --profile production up -d
```

**Features:**
- Automatic TLS certificate provisioning
- HTTP → HTTPS redirect
- Modern TLS 1.2+ only
- Security headers (HSTS, X-Frame-Options, etc.)

For manual TLS termination, see [docs/security.md](docs/security.md).

### Database Backup

The CLI includes a backup command for SQLite database protection:

```bash
# Local backup
pisovereign-cli backup --output /backup/pisovereign-$(date +%Y%m%d).db

# Backup to S3-compatible storage
pisovereign-cli backup \
  --s3-bucket my-backups \
  --s3-region eu-central-1 \
  --s3-endpoint https://s3.example.com \
  --s3-access-key "$AWS_ACCESS_KEY_ID" \
  --s3-secret-key "$AWS_SECRET_ACCESS_KEY"
```

**Recommended backup strategy:**
- Daily backups with 7-day retention
- Weekly backups with 4-week retention
- Monthly backups with 12-month retention

Example cron job:
```bash
# Daily backup at 2 AM
0 2 * * * /usr/local/bin/pisovereign-cli backup --output /backup/daily/db-$(date +\%Y\%m\%d).db
```

### Monitoring Stack

PiSovereign exposes comprehensive metrics for production monitoring:

**Prometheus Metrics (`/metrics/prometheus`):**
- `http_requests_total` – Request counts by status
- `http_response_time_p50/p90/p99_ms` – Latency percentiles
- `inference_requests_total` – Inference success/failure
- `inference_time_ms_bucket` – Inference latency histogram

**Health Endpoints:**
- `/health` – Liveness probe (always OK if running)
- `/ready` – Readiness with inference engine status
- `/ready/all` – Extended health with latency percentiles

**Grafana Dashboard:**

Import the pre-built dashboard from `grafana/dashboards/pisovereign.json`:

```bash
# Start monitoring stack
docker compose up -d grafana prometheus

# Access Grafana at http://localhost:3001 (admin/admin)
```

**Log Aggregation:**

For centralized logging with Loki:

```bash
# Configure promtail (see grafana/promtail.yml)
docker compose up -d loki promtail

# JSON logging enabled by default in production
export PISOVEREIGN_LOG_FORMAT=json
```

Log rotation is configured in `grafana/logrotate.d/pisovereign`.

### Scheduled Tasks

PiSovereign includes a cron-based scheduler for recurring tasks:

```rust
// Example: Weather refresh every 30 minutes
scheduler.add_task("weather-refresh", "0 */30 * * * *", || async {
    // Fetch and cache weather data
    Ok(())
}).await?;

// Example: Daily briefing at 7 AM
scheduler.add_task("morning-briefing", "0 0 7 * * *", || async {
    // Generate and send briefing
    Ok(())
}).await?;
```

**Predefined schedules:**
- `EVERY_30_MINUTES` – Weather data refresh
- `EVERY_15_MINUTES` – Calendar sync
- `DAILY_7AM` – Morning briefing
- `DAILY_MIDNIGHT` – Database backup

## Voice Messages (STT/TTS)

PiSovereign supports bidirectional voice communication via WhatsApp:

### Speech-to-Text (STT)

Receive voice messages from WhatsApp users, transcribe them using OpenAI Whisper, and process the text through your AI assistant:

```
User sends 🎤 voice message → Whisper transcription → AI response → Text reply
```

### Text-to-Speech (TTS)

Optionally respond with audio messages using OpenAI TTS:

```
AI text response → TTS synthesis → 🔊 Audio message to user
```

### Configuration

Add to `config.toml`:

```toml
[speech]
api_key = "sk-your-openai-key"         # Required: OpenAI API key
base_url = "https://api.openai.com/v1" # Optional: API endpoint
stt_model = "whisper-1"                # Optional: STT model
tts_model = "tts-1"                    # Optional: TTS model (tts-1, tts-1-hd)
default_voice = "nova"                 # Optional: nova, alloy, echo, fable, onyx, shimmer
response_format = "text"               # Optional: text or audio
timeout_ms = 30000                     # Optional: API timeout
max_audio_duration_ms = 120000         # Optional: Max voice message length (2 min)
```

Or via environment variables:

```bash
export PISOVEREIGN_SPEECH_API_KEY="sk-your-openai-key"
export PISOVEREIGN_SPEECH_DEFAULT_VOICE="nova"
export PISOVEREIGN_SPEECH_RESPONSE_FORMAT="audio"  # Enable TTS responses
```

### Supported Audio Formats

- **Input (WhatsApp → Whisper)**: OGG/Opus, MP3, WAV, FLAC, M4A
- **Output (TTS → WhatsApp)**: MP3 (auto-converted to OGG/Opus for WhatsApp)

### Requirements

- **FFmpeg**: Required for audio format conversion
  ```bash
  # On macOS
  brew install ffmpeg

  # On Debian/Ubuntu/Raspberry Pi OS
  sudo apt install ffmpeg
  ```

- **OpenAI API Key**: Required for Whisper and TTS APIs

### Architecture

The voice message pipeline follows Clean Architecture:

```
WhatsApp Webhook
    ↓
[integration_whatsapp] Media Download
    ↓
[ai_speech] Audio Converter (FFmpeg)
    ↓
[ai_speech] OpenAI Whisper (STT)
    ↓
[application] VoiceMessageService
    ↓
[ai_core] LLM Processing
    ↓
[ai_speech] OpenAI TTS (optional)
    ↓
[integration_whatsapp] Media Upload
    ↓
WhatsApp Response (text or audio)
```

## Performance Features

### Multi-Layer Caching

PiSovereign uses a two-tier caching system optimized for Raspberry Pi 5:

- **L1 Cache (Moka)**: In-memory, sub-millisecond access
- **L2 Cache (Sled)**: Persistent embedded store, survives restarts

LLM responses are cached with content-aware TTL:
- Dynamic content (briefings, email summaries): 1 hour
- Stable content (system prompts, help text): 24 hours

### Async Database

SQLite database operations use `sqlx` for non-blocking async I/O:
- Connection pooling for concurrent requests
- WAL mode for better read/write performance
- Prepared statements for query optimization

### Monitoring

Prometheus metrics at `/metrics/prometheus`:
- HTTP request rates and latencies
- Inference success/failure rates
- Token generation throughput

Grafana dashboard available in `grafana/dashboards/pisovereign.json`.

## Documentation

Detailed guides are available in the `docs/` directory:

- **[Deployment Guide](docs/deployment.md)** – Production deployment with Docker or native binaries
- **[Hardware Setup](docs/hardware-setup.md)** – Raspberry Pi 5 + Hailo-10H assembly and configuration
- **[Security Guide](docs/security.md)** – API key hashing, TLS, CORS, and security best practices

## Development

```bash
# Run tests
cargo test --workspace

# Format code
cargo fmt --all

# Lint
cargo clippy --workspace --all-targets

# Build documentation
cargo doc --workspace --no-deps
```

## License

MIT
