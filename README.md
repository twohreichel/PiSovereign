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

## License

MIT
