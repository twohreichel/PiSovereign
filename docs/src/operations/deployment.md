# Production Deployment

> 🚀 Deploy PiSovereign for production use

This guide covers deploying PiSovereign in production with TLS, monitoring, and high availability considerations.

## Table of Contents

- [Overview](#overview)
- [Pre-Deployment Checklist](#pre-deployment-checklist)
- [Docker Deployment](#docker-deployment)
  - [Docker Compose Setup](#docker-compose-setup)
  - [Container Configuration](#container-configuration)
  - [Multi-Architecture Builds](#multi-architecture-builds)
- [Native Binary Deployment](#native-binary-deployment)
  - [Systemd Service](#systemd-service)
  - [Binary Management](#binary-management)
- [TLS Configuration](#tls-configuration)
  - [Traefik with Let's Encrypt](#traefik-with-lets-encrypt)
  - [Manual Certificate Setup](#manual-certificate-setup)
- [Production Configuration](#production-configuration)
- [Deployment Verification](#deployment-verification)

---

## Overview

PiSovereign supports two deployment methods:

| Method | Best For | Complexity |
|--------|----------|------------|
| **Docker** | Reproducible deployments, easy updates | Medium |
| **Native Binary** | Maximum performance, minimal overhead | Low |

Recommended production architecture:

```
Internet
    │
    ▼
┌─────────────┐
│   Traefik   │ ← TLS termination, Let's Encrypt
│  (Reverse   │
│   Proxy)    │
└─────────────┘
    │ HTTP (internal)
    ▼
┌─────────────┐     ┌─────────────┐
│ PiSovereign │ ──▶ │ Hailo-      │
│   Server    │     │ Ollama      │
└─────────────┘     └─────────────┘
    │
    ▼
┌─────────────┐     ┌─────────────┐
│  Prometheus │ ──▶ │   Grafana   │
│   Metrics   │     │  Dashboard  │
└─────────────┘     └─────────────┘
```

---

## Pre-Deployment Checklist

Before deploying to production:

- [ ] **Hardware setup complete** ([Raspberry Pi Setup](../user/raspberry-pi-setup.md))
- [ ] **Security hardening applied** (SSH, firewall, fail2ban)
- [ ] **Vault configured** with production secrets ([Vault Setup](../user/vault-setup.md))
- [ ] **Domain name** pointing to your server
- [ ] **Firewall rules** for ports 80, 443 (external), 3000, 8080 (internal)
- [ ] **Backup strategy** defined
- [ ] **Monitoring** configured

---

## Docker Deployment

### Docker Compose Setup

1. **Install Docker**

```bash
# Install Docker
curl -fsSL https://get.docker.com | sh

# Add user to docker group
sudo usermod -aG docker $USER
newgrp docker

# Install Docker Compose plugin
sudo apt install docker-compose-plugin
```

2. **Create deployment directory**

```bash
sudo mkdir -p /opt/pisovereign
cd /opt/pisovereign
```

3. **Create docker-compose.yml**

```yaml
version: '3.8'

services:
  pisovereign:
    image: ghcr.io/twohreichel/pisovereign:latest
    container_name: pisovereign
    restart: unless-stopped
    ports:
      - "127.0.0.1:3000:3000"   # API (internal only)
      - "127.0.0.1:8080:8080"   # Metrics (internal only)
    volumes:
      - ./config.toml:/etc/pisovereign/config.toml:ro
      - ./data:/var/lib/pisovereign
      - /dev/hailo0:/dev/hailo0  # Hailo device passthrough
    environment:
      - PISOVEREIGN_CONFIG=/etc/pisovereign/config.toml
      - RUST_LOG=info
    devices:
      - /dev/hailo0:/dev/hailo0
    group_add:
      - hailo
    depends_on:
      - hailo-ollama
    networks:
      - pisovereign-net
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 10s

  hailo-ollama:
    image: ghcr.io/twohreichel/hailo-ollama:latest
    container_name: hailo-ollama
    restart: unless-stopped
    ports:
      - "127.0.0.1:11434:11434"
    volumes:
      - hailo-models:/models
    devices:
      - /dev/hailo0:/dev/hailo0
    group_add:
      - hailo
    networks:
      - pisovereign-net

  traefik:
    image: traefik:v3.0
    container_name: traefik
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./traefik:/etc/traefik:ro
      - ./acme:/acme
      - /var/run/docker.sock:/var/run/docker.sock:ro
    networks:
      - pisovereign-net
    labels:
      - "traefik.enable=true"

networks:
  pisovereign-net:
    driver: bridge

volumes:
  hailo-models:
```

4. **Create Traefik configuration**

```bash
mkdir -p traefik acme
```

```yaml
# traefik/traefik.yml
api:
  dashboard: false

entryPoints:
  web:
    address: ":80"
    http:
      redirections:
        entryPoint:
          to: websecure
          scheme: https
  websecure:
    address: ":443"
    http:
      tls:
        certResolver: letsencrypt

providers:
  docker:
    exposedByDefault: false
  file:
    directory: /etc/traefik/dynamic

certificatesResolvers:
  letsencrypt:
    acme:
      email: admin@example.com
      storage: /acme/acme.json
      httpChallenge:
        entryPoint: web
```

```yaml
# traefik/dynamic/pisovereign.yml
http:
  routers:
    pisovereign:
      rule: "Host(`pisovereign.example.com`)"
      service: pisovereign
      entryPoints:
        - websecure
      tls:
        certResolver: letsencrypt
      middlewares:
        - security-headers
        - rate-limit

  services:
    pisovereign:
      loadBalancer:
        servers:
          - url: "http://pisovereign:3000"

  middlewares:
    security-headers:
      headers:
        stsSeconds: 31536000
        stsIncludeSubdomains: true
        stsPreload: true
        forceSTSHeader: true
        contentTypeNosniff: true
        browserXssFilter: true
        referrerPolicy: "strict-origin-when-cross-origin"
        frameDeny: true

    rate-limit:
      rateLimit:
        average: 100
        burst: 50
```

5. **Deploy**

```bash
# Set permissions
chmod 600 acme/acme.json

# Pull images
docker compose pull

# Start services
docker compose up -d

# View logs
docker compose logs -f pisovereign
```

### Container Configuration

Create production `config.toml`:

```toml
environment = "production"

[server]
host = "0.0.0.0"
port = 3000
log_format = "json"
cors_enabled = true
allowed_origins = ["https://pisovereign.example.com"]

[inference]
base_url = "http://hailo-ollama:11434"
default_model = "qwen2.5-1.5b-instruct"
timeout_ms = 120000

[database]
path = "/var/lib/pisovereign/pisovereign.db"
max_connections = 10

[security]
rate_limit_enabled = true
rate_limit_rpm = 60
min_tls_version = "1.3"

[vault]
address = "http://vault:8200"
role_id = "${VAULT_ROLE_ID}"
mount_path = "secret"

[telemetry]
enabled = true
otlp_endpoint = "http://tempo:4317"
```

### Multi-Architecture Builds

PiSovereign images support both ARM64 and AMD64:

```bash
# Pull for Raspberry Pi (ARM64)
docker pull --platform linux/arm64 ghcr.io/twohreichel/pisovereign:latest

# Pull for x86 server (AMD64)
docker pull --platform linux/amd64 ghcr.io/twohreichel/pisovereign:latest
```

---

## Native Binary Deployment

For maximum performance on Raspberry Pi, use native binaries.

### Systemd Service

1. **Install binaries**

```bash
# Build or download release
cargo build --release

# Install binaries
sudo cp target/release/pisovereign-server /usr/local/bin/
sudo cp target/release/pisovereign-cli /usr/local/bin/

# Set permissions
sudo chmod 755 /usr/local/bin/pisovereign-*
```

2. **Create service user**

```bash
sudo useradd -r -s /bin/false pisovereign
sudo mkdir -p /var/lib/pisovereign /etc/pisovereign
sudo chown pisovereign:pisovereign /var/lib/pisovereign
```

3. **Create systemd service**

```bash
sudo nano /etc/systemd/system/pisovereign.service
```

```ini
[Unit]
Description=PiSovereign AI Assistant
After=network.target hailo-ollama.service
Requires=hailo-ollama.service
Documentation=https://github.com/twohreichel/PiSovereign

[Service]
Type=simple
User=pisovereign
Group=pisovereign
WorkingDirectory=/var/lib/pisovereign

ExecStart=/usr/local/bin/pisovereign-server
ExecReload=/bin/kill -HUP $MAINPID
Restart=always
RestartSec=5

# Environment
Environment="PISOVEREIGN_CONFIG=/etc/pisovereign/config.toml"
Environment="RUST_LOG=info"
EnvironmentFile=-/etc/pisovereign/env

# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/var/lib/pisovereign
PrivateTmp=yes
ProtectKernelTunables=yes
ProtectControlGroups=yes
RestrictSUIDSGID=yes
CapabilityBoundingSet=
AmbientCapabilities=
SystemCallFilter=@system-service
SystemCallErrorNumber=EPERM

# Resource limits
MemoryMax=1G
TasksMax=100
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
```

4. **Enable and start**

```bash
sudo systemctl daemon-reload
sudo systemctl enable pisovereign
sudo systemctl start pisovereign

# Check status
sudo systemctl status pisovereign
```

### Binary Management

**Update process**:

```bash
# Download new version
cd /tmp
wget https://github.com/twohreichel/PiSovereign/releases/download/v0.2.0/pisovereign-linux-arm64.tar.gz
tar -xzf pisovereign-linux-arm64.tar.gz

# Backup and replace
sudo cp /usr/local/bin/pisovereign-server /usr/local/bin/pisovereign-server.bak
sudo cp pisovereign-server /usr/local/bin/

# Restart service
sudo systemctl restart pisovereign
```

**Rollback**:

```bash
sudo cp /usr/local/bin/pisovereign-server.bak /usr/local/bin/pisovereign-server
sudo systemctl restart pisovereign
```

---

## TLS Configuration

### Traefik with Let's Encrypt

Already configured in the Docker Compose setup above. Key points:

1. **DNS A record** pointing to your server's IP
2. **Ports 80 and 443** open in firewall
3. **Valid email** for Let's Encrypt notifications

Certificate auto-renewal is handled by Traefik.

### Manual Certificate Setup

For custom certificates without Traefik:

1. **Install Certbot**

```bash
sudo apt install certbot
```

2. **Obtain certificate**

```bash
sudo certbot certonly --standalone -d pisovereign.example.com
```

3. **Configure Nginx**

```bash
sudo apt install nginx
sudo nano /etc/nginx/sites-available/pisovereign
```

```nginx
server {
    listen 80;
    server_name pisovereign.example.com;
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    server_name pisovereign.example.com;

    ssl_certificate /etc/letsencrypt/live/pisovereign.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/pisovereign.example.com/privkey.pem;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256;
    ssl_prefer_server_ciphers off;

    # Security headers
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;
    add_header X-Frame-Options DENY always;
    add_header X-Content-Type-Options nosniff always;

    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # SSE support for streaming
    location /v1/chat/stream {
        proxy_pass http://127.0.0.1:3000;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_buffering off;
        proxy_cache off;
    }
}
```

```bash
sudo ln -s /etc/nginx/sites-available/pisovereign /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

4. **Auto-renewal**

```bash
# Test renewal
sudo certbot renew --dry-run

# Cron for renewal (usually auto-configured)
sudo crontab -l | grep certbot
# 0 0,12 * * * certbot renew --quiet
```

---

## Production Configuration

Complete production `config.toml`:

```toml
# Production configuration
environment = "production"

[server]
host = "127.0.0.1"  # Behind reverse proxy
port = 3000
cors_enabled = true
allowed_origins = ["https://pisovereign.example.com"]
shutdown_timeout_secs = 30
log_format = "json"

[inference]
base_url = "http://localhost:11434"
default_model = "qwen2.5-1.5b-instruct"
timeout_ms = 120000
max_tokens = 2048
temperature = 0.7

[security]
rate_limit_enabled = true
rate_limit_rpm = 60
tls_verify_certs = true
min_tls_version = "1.3"
connection_timeout_secs = 30

[database]
path = "/var/lib/pisovereign/pisovereign.db"
max_connections = 10
run_migrations = true

[cache]
enabled = true
ttl_short_secs = 300
ttl_medium_secs = 3600
ttl_long_secs = 86400
l1_max_entries = 10000

[vault]
address = "https://vault.internal:8200"
mount_path = "secret"
timeout_secs = 5

[telemetry]
enabled = true
otlp_endpoint = "http://tempo:4317"
sample_ratio = 0.1
graceful_fallback = true

[degraded_mode]
enabled = true
unavailable_message = "Service temporarily unavailable. Please try again."
failure_threshold = 3
success_threshold = 2

[health]
global_timeout_secs = 5
```

---

## Deployment Verification

After deployment, verify everything is working:

```bash
# 1. Check service status
sudo systemctl status pisovereign hailo-ollama

# 2. Check health endpoint
curl https://pisovereign.example.com/health

# 3. Check readiness
curl https://pisovereign.example.com/ready/all | jq

# 4. Test chat endpoint
curl -X POST https://pisovereign.example.com/v1/chat \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello"}' | jq

# 5. Check TLS certificate
openssl s_client -connect pisovereign.example.com:443 -brief

# 6. Check metrics
curl http://localhost:8080/metrics/prometheus | head -20

# 7. View logs
sudo journalctl -u pisovereign -f
```

**Expected results**:
- ✅ Health returns `{"status": "ok"}`
- ✅ Ready shows all services healthy
- ✅ Chat returns AI response
- ✅ TLS shows valid certificate
- ✅ Metrics show request counters

---

## Next Steps

- [Monitoring](./monitoring.md) - Set up Prometheus and Grafana
- [Backup & Restore](./backup-restore.md) - Configure automated backups
- [Security Hardening](../security/hardening.md) - Advanced security
