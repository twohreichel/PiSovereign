# Troubleshooting

> 🔧 Solutions for common issues with PiSovereign

## Table of Contents

- [Quick Diagnostics](#quick-diagnostics)
- [Installation Issues](#installation-issues)
- [Hailo AI HAT+](#hailo-ai-hat)
- [Inference Problems](#inference-problems)
- [Network & Connectivity](#network--connectivity)
- [Database Issues](#database-issues)
- [Integration Problems](#integration-problems)
  - [WhatsApp](#whatsapp)
  - [Proton Mail](#proton-mail)
  - [CalDAV](#caldav)
- [Speech Processing](#speech-processing)
- [Performance Issues](#performance-issues)
- [Getting Help](#getting-help)

---

## Quick Diagnostics

Run these commands first to identify the problem:

```bash
# Overall system status
pisovereign-cli status

# Detailed health check
curl http://localhost:3000/ready/all | jq

# Check logs
sudo journalctl -u pisovereign -n 100 --no-pager

# Check system resources
htop
df -h
free -h
```

---

## Installation Issues

### Rust compilation fails with memory error

**Symptom**: Build fails with "out of memory" or OOM killed

**Solution**: Raspberry Pi 5 (8GB) should handle builds, but for 4GB models:

```bash
# Create swap file
sudo fallocate -l 2G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile

# Build with limited parallelism
cargo build --release -j 2

# Make swap permanent
echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
```

### Missing system dependencies

**Symptom**: Build error about missing headers or libraries

**Solution**:

```bash
# Install all required dependencies
sudo apt install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    libsqlite3-dev \
    libclang-dev \
    cmake
```

### Permission denied during installation

**Symptom**: Cannot write to `/usr/local/bin`

**Solution**:

```bash
# Use sudo for system installation
sudo cp target/release/pisovereign-* /usr/local/bin/

# Or install to user directory
mkdir -p ~/.local/bin
cp target/release/pisovereign-* ~/.local/bin/
export PATH="$HOME/.local/bin:$PATH"
```

---

## Hailo AI HAT+

### Device not detected

**Symptom**: `hailortcli scan` returns no devices

**Diagnosis**:

```bash
# Check device files
ls -la /dev/hailo*

# Check kernel module
lsmod | grep hailo

# Check PCIe
lspci | grep -i hailo

# Check dmesg for errors
dmesg | grep -i hailo
```

**Solutions**:

1. **Check physical connection**:
   - Ensure HAT+ is fully seated on GPIO pins
   - Check PCIe FPC cable is properly connected
   - Verify power supply is 27W USB-C

2. **Reinstall drivers**:
   ```bash
   sudo apt remove --purge hailo-*
   sudo apt autoremove
   sudo reboot
   sudo apt install hailo-h10-all
   sudo reboot
   ```

3. **Check user groups**:
   ```bash
   sudo usermod -aG hailo $USER
   # Log out and back in
   ```

### Hailo firmware error

**Symptom**: `hailortcli fw-control identify` fails

**Solution**:

```bash
# Reset the device
sudo hailortcli fw-control reset

# Update firmware (if available)
sudo apt update
sudo apt upgrade hailo-firmware
```

### Hailo-Ollama won't start

**Symptom**: Service fails or can't connect to localhost:11434

**Diagnosis**:

```bash
# Check service status
sudo systemctl status hailo-ollama

# Check logs
sudo journalctl -u hailo-ollama -n 50

# Test manual start
hailo-ollama serve
```

**Solutions**:

1. **Port already in use**:
   ```bash
   sudo lsof -i :11434
   # Kill existing process or change port
   ```

2. **Model not found**:
   ```bash
   hailo-ollama list
   hailo-ollama pull qwen2.5-1.5b-instruct
   ```

---

## Inference Problems

### Inference timeout

**Symptom**: Requests fail with timeout error

**Diagnosis**:

```bash
# Test inference directly
curl -X POST http://localhost:11434/api/generate \
  -d '{"model":"qwen2.5-1.5b-instruct","prompt":"Hi","stream":false}'
```

**Solutions**:

1. **Increase timeout**:
   ```toml
   [inference]
   timeout_ms = 120000  # 2 minutes
   ```

2. **Check NPU utilization**:
   ```bash
   watch -n 1 hailortcli monitor
   ```

3. **Use smaller model**:
   ```toml
   [inference]
   default_model = "llama3.2-1b-instruct"
   ```

### Model not found

**Symptom**: Error "model not found" or "invalid model"

**Solution**:

```bash
# List available models
hailo-ollama list

# Pull missing model
hailo-ollama pull qwen2.5-1.5b-instruct
```

### Poor response quality

**Symptom**: Responses are incoherent or cut off

**Solutions**:

1. **Increase max tokens**:
   ```toml
   [inference]
   max_tokens = 4096
   ```

2. **Adjust temperature**:
   ```toml
   [inference]
   temperature = 0.5  # More focused
   ```

3. **Use model selector for complex tasks**:
   ```toml
   [model_selector]
   large_model = "qwen2.5-7b-instruct"
   complexity_word_threshold = 50
   ```

---

## Network & Connectivity

### Port already in use

**Symptom**: "Address already in use" error

**Solution**:

```bash
# Find process using port
sudo lsof -i :3000
sudo kill <PID>

# Or use different port
PISOVEREIGN_SERVER_PORT=8080 pisovereign-server
```

### Connection refused

**Symptom**: Cannot connect to API endpoints

**Diagnosis**:

```bash
# Check service is running
sudo systemctl status pisovereign

# Check listening ports
sudo ss -tlnp | grep pisovereign

# Test local connection
curl -v http://127.0.0.1:3000/health
```

**Solutions**:

1. **Check bind address**:
   ```toml
   [server]
   host = "0.0.0.0"  # All interfaces
   ```

2. **Check firewall**:
   ```bash
   sudo ufw status
   sudo ufw allow 3000/tcp
   ```

### TLS/SSL errors

**Symptom**: Certificate verification failed

**Solutions**:

1. **Development (temporary)**:
   ```toml
   [security]
   tls_verify_certs = false  # NOT for production
   ```

2. **Production**: Ensure valid certificates or add custom CA:
   ```toml
   [proton.tls]
   ca_cert_path = "/path/to/ca.pem"
   ```

---

## Database Issues

### Database locked

**Symptom**: "database is locked" error

**Cause**: Multiple writers to SQLite

**Solutions**:

1. **Ensure single instance**:
   ```bash
   pgrep -f pisovereign-server
   # Kill duplicates if found
   ```

2. **Check file permissions**:
   ```bash
   ls -la /var/lib/pisovereign/
   # Should be owned by pi:pi
   ```

3. **Enable WAL mode** (default, but verify):
   ```bash
   sqlite3 /var/lib/pisovereign/pisovereign.db "PRAGMA journal_mode;"
   # Should return "wal"
   ```

### Migration failed

**Symptom**: Startup fails with migration error

**Solution**:

```bash
# Backup current database
cp /var/lib/pisovereign/pisovereign.db ~/pisovereign-backup.db

# Reset database (LOSES DATA)
rm /var/lib/pisovereign/pisovereign.db
sudo systemctl restart pisovereign
```

### Database corruption

**Symptom**: "database disk image is malformed"

**Solution**:

```bash
# Try to recover
sqlite3 /var/lib/pisovereign/pisovereign.db ".recover" | \
  sqlite3 /var/lib/pisovereign/pisovereign-recovered.db

# Or restore from backup
pisovereign-cli restore --input /path/to/backup.db
```

---

## Integration Problems

### WhatsApp

#### Webhook verification failed

**Symptom**: Meta shows webhook verification error

**Checklist**:
1. URL is publicly accessible (test with curl from external network)
2. `verify_token` in config matches Meta console
3. HTTPS is properly configured
4. No firewall blocking port 443

#### Messages not received

**Diagnosis**:

```bash
# Check webhook logs
sudo journalctl -u pisovereign | grep -i whatsapp
```

**Solutions**:
1. Verify webhook is subscribed to `messages` field
2. Check phone number is whitelisted (for test numbers)
3. Verify signature validation: `signature_required = true`

### Proton Mail

#### Connection refused to Bridge

**Symptom**: Cannot connect to IMAP/SMTP ports

**Diagnosis**:

```bash
# Check Bridge is running
systemctl status protonmail-bridge

# Check ports
sudo ss -tlnp | grep -E '1143|1025'
```

**Solutions**:
1. Start Bridge: `protonmail-bridge --noninteractive`
2. Ensure user is logged in (may need GUI for initial setup)
3. Check firewall allows local connections

#### Authentication failed

**Symptom**: Invalid credentials error

**Cause**: Using Proton account password instead of Bridge password

**Solution**: Use the password shown in Proton Bridge UI, not your account password

### CalDAV

#### 401 Unauthorized

**Symptom**: Authentication fails

**Diagnosis**:

```bash
# Test with curl
curl -u username:password \
  http://localhost:8080/dav.php/calendars/username/
```

**Solutions**:
1. Verify username/password
2. Check user exists in CalDAV server
3. URL might need trailing slash

#### 404 Not Found

**Symptom**: Calendar not found

**Solution**: Verify calendar path:

```bash
# List calendars
curl -u username:password \
  -X PROPFIND \
  http://localhost:8080/dav.php/calendars/username/
```

---

## Speech Processing

### Local STT (Whisper) fails

**Symptom**: Transcription error or timeout

**Diagnosis**:

```bash
# Test whisper directly
whisper-cpp -m /usr/local/share/whisper/ggml-base.bin -f test.wav
```

**Solutions**:

1. **Check model exists**:
   ```bash
   ls -la /usr/local/share/whisper/
   ```

2. **Verify audio format**:
   ```bash
   ffprobe input.ogg
   # Should be mono, 16kHz for best results
   ```

3. **Check memory**:
   ```bash
   free -h
   # Whisper needs ~500MB RAM
   ```

### Local TTS (Piper) fails

**Symptom**: No audio output or garbled audio

**Diagnosis**:

```bash
# Test Piper directly
echo "Hello world" | piper \
  --model /usr/local/share/piper/voices/en_US-lessac-medium.onnx \
  --output_file test.wav
```

**Solutions**:

1. **Check voice model**:
   ```bash
   ls -la /usr/local/share/piper/voices/
   ```

2. **Verify ONNX runtime**:
   ```bash
   piper --help  # Should show version
   ```

### FFmpeg conversion errors

**Symptom**: Audio format conversion fails

**Solution**:

```bash
# Install/update FFmpeg
sudo apt install -y ffmpeg

# Test conversion
ffmpeg -i input.ogg -ar 16000 -ac 1 output.wav
```

---

## Performance Issues

### High CPU usage

**Diagnosis**:

```bash
htop
# Look for pisovereign or hailo processes
```

**Solutions**:

1. **Limit concurrent requests**:
   ```toml
   [server]
   max_connections = 10
   ```

2. **Enable caching**:
   ```toml
   [cache]
   enabled = true
   l1_max_entries = 5000
   ```

### High memory usage

**Diagnosis**:

```bash
free -h
ps aux --sort=-%mem | head -10
```

**Solutions**:

1. **Reduce cache size**:
   ```toml
   [cache]
   l1_max_entries = 1000
   ```

2. **Reduce connection pool**:
   ```toml
   [database]
   max_connections = 3
   ```

### Slow response times

**Diagnosis**:

```bash
# Check metrics
curl http://localhost:3000/metrics | grep response_time
```

**Solutions**:

1. **Check inference latency** - may need faster model
2. **Enable caching** for repeated queries
3. **Check disk I/O** - SSD recommended over SD card

---

## Getting Help

### Collect Diagnostic Information

Before reporting an issue:

```bash
# System info
uname -a
cat /etc/os-release
rustc --version

# PiSovereign version
pisovereign-cli --version

# Service status
sudo systemctl status pisovereign hailo-ollama

# Recent logs
sudo journalctl -u pisovereign --since "1 hour ago" > pisovereign-logs.txt

# Hardware info
hailortcli fw-control identify
cat /proc/cpuinfo | grep -E "model name|Hardware"
free -h
df -h
```

### Report an Issue

1. **GitHub Issues**: [github.com/twohreichel/PiSovereign/issues](https://github.com/twohreichel/PiSovereign/issues)
   - Use the issue template
   - Include diagnostic information
   - Describe steps to reproduce

2. **Security Issues**: Report privately via GitHub Security Advisories

3. **Discussions**: For questions and help, use [GitHub Discussions](https://github.com/twohreichel/PiSovereign/discussions)
