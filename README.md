# PiSovereign

🤖 Local, secure AI assistant platform for Raspberry Pi 5 + Hailo-10H AI HAT+ 2.

## Features

- **Local LLM Inference** on Hailo-10H (Qwen2.5-1.5B, Llama3.2-1B)
- **WhatsApp Control** – Send commands via message
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
