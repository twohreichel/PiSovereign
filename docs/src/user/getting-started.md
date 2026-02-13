# Getting Started

> 🚀 Get PiSovereign running in under 5 minutes

PiSovereign is deployed as a set of Docker containers using Docker Compose.
This is the only supported installation method.

## Prerequisites

- **Docker Engine 24+** with Docker Compose v2
- **8 GB RAM** recommended (4 GB minimum)
- **20 GB disk space** (models + data)
- A **domain name** with DNS pointing to your server (for HTTPS)

## Quick Start

```bash
# Clone the repository
git clone https://github.com/twohreichel/PiSovereign.git
cd PiSovereign/docker

# Create your environment file
cp .env.example .env
nano .env  # Set PISOVEREIGN_DOMAIN and TRAEFIK_ACME_EMAIL

# Start all core services
docker compose up -d

# Initialize Vault (first run only — save the output!)
docker compose exec vault /vault/init.sh

# Wait for model download to complete
docker compose logs -f ollama-init
```

## What Gets Deployed

| Service | Description |
|---------|-------------|
| **PiSovereign** | AI assistant application |
| **Traefik** | HTTPS reverse proxy with Let's Encrypt |
| **Vault** | Secret management (API keys, passwords) |
| **Ollama** | LLM inference engine |
| **Signal-CLI** | Signal messenger integration |
| **Whisper** | Speech-to-text processing |
| **Piper** | Text-to-speech synthesis |

## Post-Setup

1. **Store secrets in Vault** — See [Vault Setup](./vault-setup.md)
2. **Register Signal number** — See [Signal Setup](./signal-setup.md)
3. **Configure integrations** — See [External Services](./external-services.md)
4. **Enable monitoring** (optional) — `docker compose --profile monitoring up -d`

## Verify Installation

```bash
# Check all services are running
docker compose ps

# Test the health endpoint
curl https://your-domain.example.com/health

# Check individual services
curl https://your-domain.example.com/health/inference
curl https://your-domain.example.com/health/vault
```

## Next Steps

- [Docker Setup](./docker-setup.md) — Detailed deployment guide
- [Configuration Reference](./configuration.md) — All configuration options
- [Troubleshooting](./troubleshooting.md) — Common issues and solutions
