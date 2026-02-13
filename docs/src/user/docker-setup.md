# Docker Setup

> 🐳 Production deployment guide using Docker Compose

PiSovereign runs as a set of Docker containers orchestrated by Docker Compose.
This is the recommended and only supported deployment method.

## Prerequisites

- Docker Engine 24+ and Docker Compose v2
- 4 GB+ RAM (8 GB recommended)
- 20 GB+ free disk space

Install Docker if not already installed:

```bash
# Raspberry Pi / Debian / Ubuntu
curl -fsSL https://get.docker.com | sudo sh
sudo usermod -aG docker $USER
# Log out and back in

# macOS
brew install --cask docker
```

## Quick Start

```bash
# 1. Clone the repository
git clone https://github.com/twohreichel/PiSovereign.git
cd PiSovereign/docker

# 2. Configure environment
cp .env.example .env
# Edit .env with your domain and email for TLS certificates
nano .env

# 3. Start core services
docker compose up -d

# 4. Initialize Vault (first time only)
docker compose exec vault /vault/init.sh
# Save the unseal key and root token printed to stdout!

# 5. Wait for Ollama model download
docker compose logs -f ollama-init
```

PiSovereign is now running at `https://your-domain.example.com`.

## Architecture

The deployment consists of these core services:

| Service | Purpose | Port | URL |
|---------|---------|------|-----|
| **pisovereign** | Main application server | 3000 (internal) | `http://localhost/` via Traefik |
| **traefik** | Reverse proxy + TLS | 80, 443 | `http://localhost:80` |
| **vault** | Secret management | 8200 (internal) | Internal only |
| **ollama** | LLM inference engine | 11434 (internal) | Internal only |
| **signal-cli** | Signal messenger daemon | Unix socket | Internal only |
| **whisper** | Speech-to-text (STT) | 8081 (internal) | Internal only |
| **piper** | Text-to-speech (TTS) | 8082 (internal) | Internal only |

### Monitoring Stack (profile: `monitoring`)

| Service | Purpose | Port | URL |
|---------|---------|------|-----|
| **prometheus** | Metrics collection & alerting | 9090 | `http://localhost:9090` |
| **grafana** | Dashboards & visualization | 3000 (internal) | `http://localhost/grafana` via Traefik |
| **loki** | Log aggregation | 3100 (internal) | Internal only |
| **promtail** | Log shipping agent | — | Internal only |
| **node-exporter** | Host metrics exporter | 9100 (internal) | Internal only |
| **otel-collector** | OpenTelemetry Collector | 4317/4318 (internal) | Internal only |

### CalDAV Server (profile: `caldav`)

| Service | Purpose | Port | URL |
|---------|---------|------|-----|
| **baikal** | CalDAV/CardDAV server | 80 (internal) | `http://localhost/caldav` via Traefik |

### Key Endpoints

| Endpoint | Description |
|----------|-------------|
| `http://localhost/health` | Application health check |
| `http://localhost/metrics/prometheus` | Prometheus metrics scrape target |
| `http://localhost/grafana` | Grafana dashboards (monitoring profile) |
| `http://localhost/caldav` | Baïkal CalDAV web UI (caldav profile) |
| `http://localhost:9090` | Prometheus web UI (monitoring profile) |
| `http://localhost:9090/targets` | Prometheus scrape target status |

## Configuration

### Environment Variables

Edit `docker/.env` before starting:

```bash
# Your domain (required for TLS)
PISOVEREIGN_DOMAIN=pi.example.com

# Email for Let's Encrypt certificates
TRAEFIK_ACME_EMAIL=you@example.com

# Vault root token (set after vault init)
VAULT_TOKEN=hvs.xxxxx

# Container image version
PISOVEREIGN_VERSION=latest
```

### Application Config

The main application config is at `docker/config/config.toml`.
All service hostnames use Docker network names (e.g., `ollama:11434`).

See [Configuration Reference](./configuration.md) for all options.

### Storing Secrets in Vault

After Vault initialization, store your integration secrets:

```bash
# Enter Vault container
docker compose exec vault sh

# Store WhatsApp credentials
vault kv put secret/pisovereign/whatsapp \
  access_token="your-meta-token" \
  app_secret="your-app-secret"

# Store Brave Search API key
vault kv put secret/pisovereign/websearch \
  api_key="your-brave-api-key"

# Store CalDAV credentials
vault kv put secret/pisovereign/caldav \
  password="your-caldav-password"

# Store Proton Bridge credentials
vault kv put secret/pisovereign/proton \
  password="your-bridge-password"
```

## Docker Compose Profiles

Additional services are available via profiles (see tables above for URLs):

### Monitoring Stack

```bash
docker compose --profile monitoring up -d
```

### CalDAV Server

```bash
docker compose --profile caldav up -d
```

### All Profiles

```bash
docker compose --profile monitoring --profile caldav up -d
```

## Operations

### Updating

```bash
cd docker

# Pull latest images
docker compose pull

# Recreate containers with new images
docker compose up -d
```

### Vault Management

```bash
# Check Vault status
docker compose exec vault vault status

# Unseal after restart (use key from init)
docker compose exec vault vault operator unseal <UNSEAL_KEY>

# Read a secret
docker compose exec vault vault kv get secret/pisovereign/whatsapp
```

### Logs

```bash
# Follow all logs
docker compose logs -f

# Specific service
docker compose logs -f pisovereign

# Last 100 lines
docker compose logs --tail=100 pisovereign
```

### Backup

```bash
# Stop services
docker compose down

# Backup volumes
docker run --rm -v pisovereign-data:/data -v $(pwd):/backup \
  alpine tar czf /backup/pisovereign-backup-$(date +%Y%m%d).tar.gz /data

# Restart
docker compose up -d
```

## Troubleshooting

See the [Troubleshooting](./troubleshooting.md) guide for common issues.
