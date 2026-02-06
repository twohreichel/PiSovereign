# Deployment Guide

This document describes how to deploy PiSovereign on a Raspberry Pi 5 with Hailo-10H accelerator.

## Prerequisites

### Hardware Requirements

- **Raspberry Pi 5** (8GB RAM recommended)
- **Hailo-10H M.2 AI Accelerator** (26 TOPS)
- **M.2 HAT+ for Raspberry Pi 5**
- **microSD card** (32GB+ recommended) or NVMe SSD
- **Active cooling** (required for sustained inference workloads)
- **Power supply**: Official Raspberry Pi 5 27W USB-C power supply

### Software Requirements

- **Raspberry Pi OS** (64-bit, Bookworm or later)
- **Rust** 1.83+ (see [rust-toolchain.toml](../rust-toolchain.toml))
- **Docker** and **Docker Compose** (for containerized deployment)
- **Hailo SDK** (HailoRT runtime)

## Deployment Options

### Option 1: Docker Compose (Recommended)

The easiest way to deploy PiSovereign is using Docker Compose.

1. **Clone the repository:**
   ```bash
   git clone https://github.com/your-org/pisovereign.git
   cd pisovereign
   ```

2. **Configure environment:**
   ```bash
   cp config.toml.example config.toml
   # Edit config.toml with your settings
   ```

3. **Start services:**
   ```bash
   docker compose up -d
   ```

4. **Verify deployment:**
   ```bash
   curl http://localhost:3000/health
   ```

### Option 2: Native Binary

For lower latency and direct hardware access:

1. **Build the project:**
   ```bash
   cargo build --release
   ```

2. **Set up systemd service:**
   ```bash
   sudo cp contrib/pisovereign.service /etc/systemd/system/
   sudo systemctl enable pisovereign
   sudo systemctl start pisovereign
   ```

## Configuration

### Essential Configuration

Edit `config.toml`:

```toml
[server]
host = "0.0.0.0"
port = 3000

[inference]
base_url = "http://localhost:11434"  # Ollama API endpoint
default_model = "llama3.2:3b"
max_tokens = 1024
temperature = 0.7

[security]
# IMPORTANT: Use hashed API keys in production!
# Generate with: ./target/release/pisovereign-cli hash-api-key "your-api-key"
api_keys = [
    "$argon2id$v=19$m=19456,t=2,p=1$..."
]
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `RUST_LOG` | Log level (trace, debug, info, warn, error) | `info` |
| `CONFIG_PATH` | Path to config file | `config.toml` |
| `DATABASE_URL` | SQLite database path | `pisovereign.db` |

## Monitoring

### Prometheus Metrics

PiSovereign exposes Prometheus metrics at `/metrics`:

- `pisovereign_requests_total` - Total HTTP requests
- `pisovereign_inference_duration_seconds` - Inference latency histogram
- `pisovereign_cache_hits_total` - Response cache hits

### Grafana Dashboard

Import the provided dashboard from `grafana/dashboards/pisovereign.json`.

## Production Checklist

Before deploying to production:

- [ ] **API Keys**: Use Argon2-hashed API keys (not plaintext)
- [ ] **TLS**: Configure HTTPS via reverse proxy (nginx, caddy)
- [ ] **CORS**: Restrict allowed origins in production
- [ ] **Logging**: Set appropriate log levels (`info` or `warn`)
- [ ] **Backups**: Configure database backup strategy
- [ ] **Monitoring**: Set up Prometheus + Grafana
- [ ] **Alerting**: Configure alerts for error rates

## Troubleshooting

### Hailo Accelerator Not Detected

```bash
# Check if Hailo device is visible
lspci | grep Hailo

# Verify HailoRT is installed
hailortcli version

# Check device access
ls -la /dev/hailo*
```

### High Memory Usage

- Reduce `max_tokens` in config
- Use smaller model (e.g., `llama3.2:1b`)
- Enable swap if needed

### Performance Issues

- Ensure active cooling is working
- Check CPU throttling: `vcgencmd measure_temp`
- Monitor with `htop` and `iotop`

## Updating

### Docker Deployment

```bash
docker compose pull
docker compose up -d
```

### Native Deployment

```bash
git pull
cargo build --release
sudo systemctl restart pisovereign
```

## See Also

- [Hardware Setup Guide](hardware-setup.md)
- [Security Guide](security.md)
- [API Documentation](../crates/presentation_http/src/openapi.rs)
