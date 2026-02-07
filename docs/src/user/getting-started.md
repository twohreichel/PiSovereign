# Getting Started

> 🚀 Get PiSovereign running in under 30 minutes

This guide provides a quick path to getting PiSovereign operational. For detailed setup instructions, see the comprehensive [Raspberry Pi Setup](./raspberry-pi-setup.md) guide.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Quick Installation](#quick-installation)
- [First Run](#first-run)
- [Basic Usage](#basic-usage)
- [Next Steps](#next-steps)

---

## Prerequisites

### Hardware Requirements

| Component | Requirement |
|-----------|-------------|
| **Raspberry Pi** | Raspberry Pi 5 (8 GB RAM recommended) |
| **AI Accelerator** | Hailo AI HAT+ 2 (Hailo-10H) |
| **Storage** | NVMe SSD via PCIe (recommended) or 32 GB+ SD card |
| **Power** | Official 27W USB-C Power Supply |
| **Cooling** | Active cooler (required for sustained inference) |

### Software Requirements

| Software | Version | Notes |
|----------|---------|-------|
| **Raspberry Pi OS** | Trixie (64-bit) | Lite version recommended |
| **Rust** | 1.93.0+ | Edition 2024 |
| **Git** | 2.x | For cloning the repository |
| **FFmpeg** | 5.x+ | For voice message processing |

---

## Quick Installation

### Step 1: Install System Dependencies

```bash
# Update system
sudo apt update && sudo apt upgrade -y

# Install required packages
sudo apt install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    libsqlite3-dev \
    ffmpeg \
    git

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

### Step 2: Install Hailo Packages

```bash
# Add Hailo repository
curl -fsSL https://hailo.ai/downloads/hailo-apt-key.pub | sudo gpg --dearmor -o /usr/share/keyrings/hailo.gpg
echo "deb [signed-by=/usr/share/keyrings/hailo.gpg] https://apt.hailo.ai/ bookworm main" | sudo tee /etc/apt/sources.list.d/hailo.list

# Install Hailo packages
sudo apt update
sudo apt install -y hailo-h10-all

# Verify installation
hailortcli scan
```

### Step 3: Clone and Build PiSovereign

```bash
# Clone repository
git clone https://github.com/twohreichel/PiSovereign.git
cd PiSovereign

# Build release binary
cargo build --release

# Verify build
./target/release/pisovereign-cli --version
```

### Step 4: Start Hailo-Ollama

```bash
# Start the inference server (background)
hailo-ollama serve &

# Verify it's running
curl http://localhost:11434/api/tags
```

---

## First Run

### Create Configuration

```bash
# Copy example configuration
cp config.toml config.local.toml

# Edit configuration (at minimum, review server settings)
nano config.local.toml
```

Key settings to review:

```toml
[server]
host = "0.0.0.0"
port = 3000

[inference]
base_url = "http://localhost:11434"
default_model = "qwen2.5-1.5b-instruct"
```

### Start the Server

```bash
# Run with your configuration
PISOVEREIGN_CONFIG=config.local.toml ./target/release/pisovereign-server
```

You should see:

```
INFO  pisovereign_http > Starting PiSovereign server on 0.0.0.0:3000
INFO  pisovereign_http > Inference engine: http://localhost:11434 (qwen2.5-1.5b-instruct)
INFO  pisovereign_http > Server ready
```

### Verify Installation

```bash
# Check health endpoint
curl http://localhost:3000/health

# Check readiness (includes inference engine)
curl http://localhost:3000/ready

# Send a test message
curl -X POST http://localhost:3000/v1/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello, what can you do?"}'
```

---

## Basic Usage

### CLI Commands

```bash
# Check system status
pisovereign-cli status

# Send a chat message
pisovereign-cli chat "What's the weather like?"

# Execute a command
pisovereign-cli command "briefing"

# Get help
pisovereign-cli --help
```

### API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Liveness check |
| `/ready` | GET | Readiness with inference status |
| `/v1/chat` | POST | Send chat message |
| `/v1/chat/stream` | POST | Streaming chat (SSE) |
| `/v1/commands` | POST | Execute command |

### Example: Chat API

```bash
curl -X POST http://localhost:3000/v1/chat \
  -H "Content-Type: application/json" \
  -d '{
    "message": "Create a reminder for tomorrow at 10am",
    "conversation_id": "optional-id-for-context"
  }'
```

---

## Next Steps

Now that PiSovereign is running, consider:

1. **[Complete Raspberry Pi Setup](./raspberry-pi-setup.md)**: Security hardening, SSH configuration, firewall rules

2. **[Configure Vault](./vault-setup.md)**: Secure secret management for API keys and credentials

3. **[Set Up Integrations](./external-services.md)**: WhatsApp, Proton Mail, CalDAV calendar

4. **[Review Configuration](./configuration.md)**: Customize all available options

5. **[Production Deployment](../operations/deployment.md)**: TLS, monitoring, backups

---

## Common Issues

### Hailo not detected

```bash
# Check device
ls /dev/hailo*

# Restart HailoRT
sudo systemctl restart hailort
```

### Port already in use

```bash
# Find process using port 3000
sudo lsof -i :3000

# Use different port
PISOVEREIGN_SERVER_PORT=8080 ./target/release/pisovereign-server
```

### Inference timeout

Increase timeout in `config.toml`:

```toml
[inference]
timeout_ms = 120000  # 2 minutes
```

For more solutions, see [Troubleshooting](./troubleshooting.md).
