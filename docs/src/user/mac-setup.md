# macOS Setup

> 🍎 Complete guide for setting up PiSovereign on macOS (Intel or Apple Silicon)

This guide covers installing and configuring PiSovereign on a Mac, leveraging Ollama with Metal GPU acceleration for local LLM inference.

## Table of Contents

- [Prerequisites](#prerequisites)
  - [System Requirements](#system-requirements)
  - [Required Software](#required-software)
- [Ollama Installation](#ollama-installation)
  - [Install Ollama](#install-ollama)
  - [Pull Models](#pull-models)
  - [Verify Installation](#verify-installation)
- [PiSovereign Installation](#pisovereign-installation)
  - [Install via Cargo](#install-via-cargo)
  - [Build from Source](#build-from-source)
- [Configuration](#configuration)
  - [Basic Configuration](#basic-configuration)
  - [Speech Processing (Optional)](#speech-processing-optional)
- [Running PiSovereign](#running-pisovereign)
  - [Development Mode](#development-mode)
  - [Background Service (launchd)](#background-service-launchd)
- [Troubleshooting](#troubleshooting)

---

## Prerequisites

### System Requirements

| Requirement | Minimum | Recommended |
|-------------|---------|-------------|
| **macOS Version** | 13.0 (Ventura) | 14.0+ (Sonoma) |
| **RAM** | 8 GB | 16 GB+ |
| **Free Disk Space** | 10 GB | 20 GB+ |
| **Processor** | Intel Core i5 / Apple M1 | Apple M1 Pro+ |

> 💡 **Apple Silicon Advantage**: M1/M2/M3 Macs provide excellent performance with Ollama's Metal acceleration, comparable to the Hailo NPU on Raspberry Pi.

### Required Software

Install [Homebrew](https://brew.sh) if not already present:

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

Install required packages:

```bash
# Essential packages
brew install rust ffmpeg

# Ollama for LLM inference
brew install ollama

# Optional: for local speech processing
brew install whisper-cpp
```

---

## Ollama Installation

### Install Ollama

If not installed via Homebrew:

```bash
# macOS (alternative: download from https://ollama.ai)
curl -fsSL https://ollama.com/install.sh | sh
```

Start the Ollama service:

```bash
# Start Ollama in the background
ollama serve &

# Or run as a foreground process for debugging
ollama serve
```

> 📝 **Note**: Ollama automatically uses Metal for GPU acceleration on Apple Silicon Macs.

### Pull Models

Download the recommended model:

```bash
# Recommended: Same model used on Raspberry Pi
ollama pull qwen2.5:1.5b-instruct

# Optional: Larger model for more capable responses (requires 8GB+ RAM)
ollama pull qwen2.5:3b-instruct
```

Verify the model is available:

```bash
ollama list
```

Expected output:
```
NAME                     ID              SIZE      MODIFIED
qwen2.5:1.5b-instruct    abc123def...    1.0 GB    Just now
```

### Verify Installation

Test the model:

```bash
ollama run qwen2.5:1.5b-instruct "Hello, what can you do?"
```

Check the API endpoint:

```bash
curl http://localhost:11434/api/tags | jq .
```

---

## PiSovereign Installation

### Install via Cargo

The easiest way to install PiSovereign:

```bash
# Install from crates.io (when published)
cargo install pisovereign-server pisovereign-cli
```

### Build from Source

Clone and build the project:

```bash
# Clone the repository
git clone https://github.com/twohreichel/PiSovereign.git
cd PiSovereign

# Build in release mode
cargo build --release

# Binaries are in target/release/
ls -la target/release/pisovereign-*
```

Install locally:

```bash
# Copy binaries to a PATH location
cp target/release/pisovereign-server /usr/local/bin/
cp target/release/pisovereign-cli /usr/local/bin/

# Verify installation
pisovereign-server --version
pisovereign-cli --version
```

---

## Configuration

### Basic Configuration

Create a configuration file:

```bash
# Create config directory
mkdir -p ~/.config/pisovereign

# Copy example config
cp config.toml ~/.config/pisovereign/config.toml
```

Edit `~/.config/pisovereign/config.toml`:

```toml
# PiSovereign macOS Configuration
environment = "development"

[server]
host = "127.0.0.1"
port = 3000
cors_enabled = true
log_format = "text"

[inference]
# Ollama runs on localhost:11434 by default
base_url = "http://localhost:11434"
default_model = "qwen2.5:1.5b-instruct"
timeout_ms = 60000
max_tokens = 2048
temperature = 0.7

[database]
# SQLite database location
path = "~/.config/pisovereign/pisovereign.db"
max_connections = 5
run_migrations = true

[cache]
enabled = true
ttl_short_secs = 300
ttl_medium_secs = 3600
ttl_long_secs = 86400

[degraded_mode]
enabled = true
unavailable_message = "The AI service is temporarily unavailable. Please try again."
retry_cooldown_secs = 30
failure_threshold = 3
success_threshold = 2
```

### Speech Processing (Optional)

For local speech processing (no cloud API required):

#### Install whisper.cpp

```bash
# Install via Homebrew
brew install whisper-cpp

# Download a model
mkdir -p ~/Library/Application\ Support/whisper/models
cd ~/Library/Application\ Support/whisper/models
curl -L -O https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin
```

#### Install Piper TTS

```bash
# Download Piper (check for latest release)
mkdir -p ~/Library/Application\ Support/piper
cd ~/Library/Application\ Support/piper

# Download the macOS binary and voice
# Note: Check https://github.com/rhasspy/piper/releases for the latest version
curl -L -o piper.tar.gz "https://github.com/rhasspy/piper/releases/download/v1.2.0/piper_macos_x64.tar.gz"
tar xzf piper.tar.gz
rm piper.tar.gz

# Download a voice
mkdir -p voices
cd voices
curl -L -O "https://huggingface.co/rhasspy/piper-voices/resolve/main/de/de_DE/thorsten/medium/de_DE-thorsten-medium.onnx"
curl -L -O "https://huggingface.co/rhasspy/piper-voices/resolve/main/de/de_DE/thorsten/medium/de_DE-thorsten-medium.onnx.json"
```

Add to your config:

```toml
[speech]
provider = "local"

[speech.local_stt]
executable_path = "whisper-cpp"
model_path = "~/Library/Application Support/whisper/models/ggml-base.bin"
threads = 4

[speech.local_tts]
executable_path = "~/Library/Application Support/piper/piper"
model_path = "~/Library/Application Support/piper/voices/de_DE-thorsten-medium.onnx"
```

---

## Running PiSovereign

### Development Mode

Start the server:

```bash
# Ensure Ollama is running
pgrep -x ollama || ollama serve &

# Start PiSovereign
pisovereign-server --config ~/.config/pisovereign/config.toml
```

Test the API:

```bash
# Health check
curl http://localhost:3000/health

# Chat request
curl -X POST http://localhost:3000/api/v1/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello, how are you?"}'
```

### Background Service (launchd)

Create a launch agent for automatic startup:

```bash
cat > ~/Library/LaunchAgents/com.pisovereign.server.plist << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.pisovereign.server</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/pisovereign-server</string>
        <string>--config</string>
        <string>~/.config/pisovereign/config.toml</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>~/.config/pisovereign/logs/server.log</string>
    <key>StandardErrorPath</key>
    <string>~/.config/pisovereign/logs/server.error.log</string>
    <key>WorkingDirectory</key>
    <string>~/.config/pisovereign</string>
</dict>
</plist>
EOF
```

Load the service:

```bash
# Create log directory
mkdir -p ~/.config/pisovereign/logs

# Load the launch agent
launchctl load ~/Library/LaunchAgents/com.pisovereign.server.plist

# Check status
launchctl list | grep pisovereign

# View logs
tail -f ~/.config/pisovereign/logs/server.log
```

Manage the service:

```bash
# Stop
launchctl stop com.pisovereign.server

# Start
launchctl start com.pisovereign.server

# Unload (disable autostart)
launchctl unload ~/Library/LaunchAgents/com.pisovereign.server.plist
```

---

## Troubleshooting

### Ollama Not Running

**Symptom**: Connection refused to localhost:11434

```bash
# Check if Ollama is running
pgrep -fl ollama

# Start Ollama
ollama serve &

# Check Ollama logs
cat ~/.ollama/logs/server.log
```

### Model Not Found

**Symptom**: Error about missing model

```bash
# List available models
ollama list

# Pull the required model
ollama pull qwen2.5:1.5b-instruct
```

### Permission Denied

**Symptom**: Cannot bind to port 3000

```bash
# Check what's using the port
lsof -i :3000

# Use a different port
pisovereign-server --config config.toml --port 8080
```

### Metal GPU Not Used

**Symptom**: Inference is slow on Apple Silicon

```bash
# Verify Metal support
system_profiler SPDisplaysDataType | grep Metal

# Check Ollama is using Metal
ollama run qwen2.5:1.5b-instruct "test" --verbose 2>&1 | grep -i metal
```

### Database Errors

**Symptom**: SQLite errors or migration failures

```bash
# Remove and recreate database
rm ~/.config/pisovereign/pisovereign.db
pisovereign-server --config ~/.config/pisovereign/config.toml
```

---

## Performance Comparison

| Metric | Raspberry Pi 5 + Hailo | Mac M1 | Mac M2 Pro |
|--------|------------------------|--------|------------|
| **First Token Latency** | ~200ms | ~150ms | ~80ms |
| **Tokens/Second** | 15-20 | 25-35 | 45-60 |
| **Memory Usage** | ~2GB | ~3GB | ~3GB |
| **Power Consumption** | 15W | 20W | 30W |

> 💡 **Tip**: For development, Mac provides faster iteration. For deployment, the Raspberry Pi offers a dedicated, always-on solution.

---

## Next Steps

- [Configuration Reference](./configuration.md) - Detailed configuration options
- [External Services](./external-services.md) - Integrate WhatsApp, CalDAV, etc.
- [Troubleshooting](./troubleshooting.md) - Common issues and solutions
- [Developer Guide](../developer/index.md) - Contributing and architecture
