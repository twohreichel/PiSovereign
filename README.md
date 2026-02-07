# PiSovereign

[![CI](https://img.shields.io/github/actions/workflow/status/twohreichel/PiSovereign/ci.yml?branch=main&label=CI&logo=github)](https://github.com/twohreichel/PiSovereign/actions/workflows/ci.yml)
[![Coverage](https://codecov.io/gh/twohreichel/PiSovereign/graph/badge.svg)](https://codecov.io/gh/twohreichel/PiSovereign)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Documentation](https://img.shields.io/badge/docs-online-blue)](https://twohreichel.github.io/PiSovereign/)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange?logo=rust)](https://www.rust-lang.org/)
[![Raspberry Pi](https://img.shields.io/badge/Raspberry%20Pi-5-C51A4A?logo=raspberrypi)](https://www.raspberrypi.com/)
[![AI](https://img.shields.io/badge/AI-Hailo--10H-blueviolet?logo=ai)](https://hailo.ai/)

🤖 **Local, private AI assistant for Raspberry Pi 5 + Hailo-10H AI HAT+ 2**

Run your own AI assistant with 100% local inference—no cloud required. Control via WhatsApp, voice messages, calendar, and email integration.

## ✨ Features

- **Local LLM Inference** on Hailo-10H NPU (26 TOPS)
- **WhatsApp Control** – Send commands via message
- **Voice Messages** – Local STT/TTS with cloud fallback
- **Calendar & Email** – CalDAV + Proton Mail integration
- **EU/GDPR Compliant** – All processing on your hardware

## 🚀 Quick Start

\`\`\`bash
git clone https://github.com/twohreichel/PiSovereign.git && cd PiSovereign
cargo build --release
./target/release/pisovereign-server
\`\`\`

## 📚 Documentation

| Guide | Description |
|-------|-------------|
| [**Getting Started**](https://twohreichel.github.io/PiSovereign/user/getting-started.html) | 30-minute setup guide |
| [**Raspberry Pi Setup**](https://twohreichel.github.io/PiSovereign/user/raspberry-pi-setup.html) | Complete hardware setup with Hailo |
| [**Configuration**](https://twohreichel.github.io/PiSovereign/user/configuration.html) | All config.toml options |
| [**API Reference**](https://twohreichel.github.io/PiSovereign/api/) | Rustdoc API documentation |
| [**Architecture**](https://twohreichel.github.io/PiSovereign/developer/architecture.html) | Clean Architecture overview |

## 💖 Support

If you find PiSovereign useful, consider [sponsoring the project](https://github.com/sponsors/twohreichel).

## 📄 License

MIT © [Andreas Reichel](https://github.com/twohreichel)
