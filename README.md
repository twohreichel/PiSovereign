# PiSovereign

🤖 Lokale, sichere KI-Assistenz-Plattform für Raspberry Pi 5 + Hailo-10H AI HAT+ 2.

## Features

- **Lokale LLM-Inferenz** auf Hailo-10H (Qwen2.5-1.5B, Llama3.2-1B)
- **WhatsApp-Steuerung** – Befehle per Nachricht senden
- **Kalender-Integration** (CalDAV: Baïkal, Radicale)
- **E-Mail-Integration** (Proton Mail Bridge)
- **EU/DSGVO-konform** – Alles lokal, europäische Dienste

## Quick Start

### Voraussetzungen

- Raspberry Pi 5 (8 GB RAM)
- Hailo AI HAT+ 2 (Hailo-10H)
- Raspberry Pi OS Trixie (64-bit)
- Rust 1.85+ (Edition 2024)

### Installation

```bash
# 1. Repository klonen
git clone https://github.com/andreasreichel/PiSovereign.git
cd PiSovereign

# 2. Hailo-Pakete installieren (auf Pi)
sudo apt install hailo-h10-all

# 3. Hailo-Ollama starten
hailo-ollama &

# 4. PiSovereign bauen
cargo build --release

# 5. Server starten
./target/release/pisovereign-server
```

### CLI Nutzung

```bash
# Status abfragen
pisovereign-cli status

# Chat-Nachricht senden
pisovereign-cli chat "Was ist das Wetter morgen?"

# Befehl ausführen
pisovereign-cli command "briefing"
```

## API Endpoints

| Endpoint | Methode | Beschreibung |
|----------|---------|--------------|
| `/health` | GET | Liveness-Check |
| `/ready` | GET | Readiness-Check mit Hailo-Status |
| `/v1/chat` | POST | Chat-Nachricht senden |
| `/v1/chat/stream` | POST | Streaming-Chat (SSE) |
| `/v1/commands` | POST | Befehl ausführen |
| `/v1/commands/parse` | POST | Befehl parsen ohne Ausführung |
| `/v1/system/status` | GET | Systemstatus |
| `/v1/system/models` | GET | Verfügbare Modelle |

## Projekt-Struktur

```
crates/
├── domain/              # Kern-Entities, Value Objects, Commands
├── application/         # Use Cases, Services, Ports
├── infrastructure/      # Adapter (Hailo, DB, etc.)
├── ai_core/            # Inferenz-Engine, Hailo-Client
├── presentation_http/   # HTTP-API (Axum)
├── presentation_cli/    # CLI-Tool
├── integration_whatsapp/# WhatsApp Business API
├── integration_caldav/  # CalDAV-Client
└── integration_proton/  # Proton Mail Bridge
```

## Konfiguration

Umgebungsvariablen oder `config.toml`:

```bash
export PISOVEREIGN_SERVER_PORT=3000
export PISOVEREIGN_INFERENCE_BASE_URL=http://localhost:11434
export PISOVEREIGN_INFERENCE_DEFAULT_MODEL=qwen2.5-1.5b-instruct
```

## Lizenz

MIT
