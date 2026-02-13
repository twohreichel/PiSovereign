# PiSovereign

> [!WARNING]
> **This project is in early beta and under active development.**
> It is not production-ready, has not been thoroughly tested, and may contain bugs or incomplete features.
> Use at your own risk. Breaking changes may occur without notice.

[![Coverage](https://img.shields.io/codecov/c/github/twohreichel/PiSovereign?logo=codecov&label=coverage)](https://codecov.io/gh/twohreichel/PiSovereign)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Documentation](https://img.shields.io/badge/docs-online-blue)](https://twohreichel.github.io/PiSovereign/)
[![Rust](https://img.shields.io/badge/Rust-1.93+-orange?logo=rust)](https://www.rust-lang.org/)
[![Raspberry Pi](https://img.shields.io/badge/Raspberry%20Pi-5-C51A4A?logo=raspberrypi)](https://www.raspberrypi.com/)
[![Docker](https://img.shields.io/badge/Docker-Compose-2496ED?logo=docker)](https://docs.docker.com/compose/)
[![AI](https://img.shields.io/badge/AI-Hailo--10H-blueviolet?logo=ai)](https://hailo.ai/)

🤖 **Local, private AI assistant for Raspberry Pi 5**

Run your own AI assistant with 100% local inference — no cloud required.
Control via Signal, WhatsApp, or HTTP API with calendar, email, and voice integration.

**📖 [Full Documentation](https://twohreichel.github.io/PiSovereign/)**

## ✨ Features

- **Local LLM Inference** on Hailo-10H NPU (26 TOPS) via Ollama
- **Docker Compose** — One-command production deployment
- **Signal & WhatsApp** — Control via messenger
- **Voice Messages** — Local STT (whisper.cpp) + TTS (Piper)
- **Calendar & Email** — CalDAV + Proton Mail integration
- **Vault Secrets** — HashiCorp Vault for credential management
- **Monitoring** — Prometheus, Grafana, Loki, OpenTelemetry
- **EU/GDPR Compliant** — All processing on your hardware

## 🚀 Quick Start

```bash
# 1. Clone
git clone https://github.com/twohreichel/PiSovereign.git
cd PiSovereign/docker

# 2. Configure
cp .env.example .env
nano .env  # Set your domain and email for TLS

# 3. Deploy
docker compose up -d

# 4. Initialize Vault (first time — save the printed unseal key!)
docker compose exec vault /vault/init.sh
```

PiSovereign is now running at `https://your-domain.example.com`.

### Post-Setup

```bash
# Store integration secrets in Vault
docker compose exec vault vault kv put secret/pisovereign/whatsapp \
  access_token="..." app_secret="..."

# Enable monitoring (optional)
docker compose --profile monitoring up -d

# Verify
curl https://your-domain.example.com/health
```

## 🏗️ Architecture

```
┌──────────────────────────────────────────────────────┐
│                     Traefik (TLS)                    │
├──────────┬──────────┬──────────┬──────────┬──────────┤
│PiSovereign│  Ollama  │Signal-CLI│ Whisper  │  Piper   │
│  (Rust)  │  (LLM)   │ (Msgs)  │  (STT)   │  (TTS)   │
├──────────┴──────────┴──────────┴──────────┴──────────┤
│                  Vault (Secrets)                      │
├──────────────────────────────────────────────────────┤
│              Docker Compose Network                   │
└──────────────────────────────────────────────────────┘
```

### Docker Compose Profiles

| Profile | Services |
|---------|----------|
| **(core)** | PiSovereign, Traefik, Vault, Ollama, Signal-CLI, Whisper, Piper |
| `monitoring` | Prometheus, Grafana, Loki, Promtail, Node Exporter, OTel Collector |
| `caldav` | Baïkal CalDAV/CardDAV server |

```bash
# All profiles
docker compose --profile monitoring --profile caldav up -d
```

## 📚 Documentation

| Guide | Description |
|-------|-------------|
| [**Getting Started**](https://twohreichel.github.io/PiSovereign/user/getting-started.html) | 5-minute Docker deployment |
| [**Hardware Setup**](https://twohreichel.github.io/PiSovereign/user/hardware-setup.html) | Raspberry Pi 5 + Hailo assembly |
| [**Docker Setup**](https://twohreichel.github.io/PiSovereign/user/docker-setup.html) | Detailed deployment guide |
| [**Vault Setup**](https://twohreichel.github.io/PiSovereign/user/vault-setup.html) | Secret management |
| [**Configuration**](https://twohreichel.github.io/PiSovereign/user/configuration.html) | All config.toml options |
| [**Architecture**](https://twohreichel.github.io/PiSovereign/developer/architecture.html) | Clean Architecture overview |
| [**Monitoring**](https://twohreichel.github.io/PiSovereign/operations/monitoring.html) | Grafana dashboards & alerts |

## 🛠️ Development

```bash
# Install just task runner
cargo install just

# Run all lints
just lint

# Run tests
just test

# Build release
just build-release

# Full quality check
just quality
```

## 💖 Support

If you find PiSovereign useful, consider [sponsoring the project](https://github.com/sponsors/twohreichel).

## 📄 License

MIT © [Andreas Reichel](https://github.com/twohreichel)
