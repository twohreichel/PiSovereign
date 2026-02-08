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
[![macOS](https://img.shields.io/badge/macOS-Sonoma+-000000?logo=apple)](https://www.apple.com/macos/)
[![AI](https://img.shields.io/badge/AI-Hailo--10H-blueviolet?logo=ai)](https://hailo.ai/)

🤖 **Local, private AI assistant for Raspberry Pi 5 or macOS**

Run your own AI assistant with 100% local inference—no cloud required. Control via WhatsApp, voice messages, calendar, and email integration.

**📖 [Full Documentation](https://twohreichel.github.io/PiSovereign/)**

## ✨ Features

- **Local LLM Inference** on Hailo-10H NPU (26 TOPS) or Ollama with Metal
- **Multi-Platform** – Raspberry Pi (production) or macOS (development)
- **WhatsApp Control** – Send commands via message
- **Voice Messages** – Local STT/TTS with cloud fallback
- **Calendar & Email** – CalDAV + Proton Mail integration
- **EU/GDPR Compliant** – All processing on your hardware

## 🚀 Quick Start

Get up and running in minutes with our automated setup scripts.

### Raspberry Pi 5

```bash
# Native build (recommended for production)
curl -fsSL https://raw.githubusercontent.com/twohreichel/PiSovereign/main/scripts/setup-pi.sh | sudo bash

# Or Docker deployment
curl -fsSL https://raw.githubusercontent.com/twohreichel/PiSovereign/main/scripts/setup-pi.sh | sudo bash -s -- --docker
```

**Options:**
| Flag | Description |
|------|-------------|
| `--native` | Build from source (default, optimized for ARM64) |
| `--docker` | Use Docker Compose deployment |
| `--branch <name>` | Use specific branch (default: main) |
| `--skip-security` | Skip security hardening |
| `--skip-build` | Skip compilation (use existing binaries) |
| `-h, --help` | Show help message |

**What it does:**
- Installs Docker, Hailo SDK, whisper.cpp, and Piper TTS
- **Native mode:** Installs Rust, builds optimized ARM64 binaries, sets up systemd service
- **Docker mode:** Deploys via Docker Compose with TLS (Traefik)
- Configures security hardening (SSH, UFW firewall, Fail2ban)
- Sets up automatic updates (rebuilds from source or pulls images)
- Interactively configures your `config.toml`

### macOS (Development)

```bash
# Docker deployment (recommended for Mac)
curl -fsSL https://raw.githubusercontent.com/twohreichel/PiSovereign/main/scripts/setup-mac.sh | bash

# Or native build
curl -fsSL https://raw.githubusercontent.com/twohreichel/PiSovereign/main/scripts/setup-mac.sh | bash -s -- --native
```

**Options:**
| Flag | Description |
|------|-------------|
| `--docker` | Use Docker Compose (default on macOS) |
| `--native` | Build from source for local development |
| `--branch <name>` | Use specific branch (default: main) |
| `--skip-build` | Skip compilation (use existing binaries) |
| `-h, --help` | Show help message |

**What it does:**
- Installs Homebrew dependencies (Ollama, whisper-cpp, FFmpeg)
- Downloads Piper TTS and German voice model
- Pulls the `qwen2.5:1.5b` LLM
- **Native mode:** Installs Rust, builds native binaries, sets up launchd service
- **Docker mode:** Deploys via Docker Compose
- Sets up automatic weekly updates via launchd

> [!TIP]
> For manual installation or customization, see the [Getting Started](https://twohreichel.github.io/PiSovereign/user/getting-started.html) guide.

## 📚 Documentation

| Guide | Description |
|-------|-------------|
| [**Getting Started**](https://twohreichel.github.io/PiSovereign/user/getting-started.html) | 30-minute setup guide |
| [**Raspberry Pi Setup**](https://twohreichel.github.io/PiSovereign/user/raspberry-pi-setup.html) | Complete hardware setup with Hailo |
| [**macOS Setup**](https://twohreichel.github.io/PiSovereign/user/mac-setup.html) | Installation guide for Mac |
| [**Configuration**](https://twohreichel.github.io/PiSovereign/user/configuration.html) | All config.toml options |
| [**API Reference**](https://twohreichel.github.io/PiSovereign/api/) | Rustdoc API documentation |
| [**Architecture**](https://twohreichel.github.io/PiSovereign/developer/architecture.html) | Clean Architecture overview |

## 💖 Support

If you find PiSovereign useful, consider [sponsoring the project](https://github.com/sponsors/twohreichel).

## 📄 License

MIT © [Andreas Reichel](https://github.com/twohreichel)
