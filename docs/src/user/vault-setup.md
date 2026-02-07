# HashiCorp Vault Setup

> 🔐 Secure secret management for PiSovereign using HashiCorp Vault

This guide covers installing, configuring, and integrating HashiCorp Vault with PiSovereign for secure credential storage.

## Table of Contents

- [Overview](#overview)
- [Installation](#installation)
  - [Docker Installation](#docker-installation)
  - [Native Installation](#native-installation)
- [Configuration](#configuration)
  - [Initialize Vault](#initialize-vault)
  - [Unseal Process](#unseal-process)
  - [Enable KV Secrets Engine](#enable-kv-secrets-engine)
- [AppRole Authentication](#approle-authentication)
  - [Create Policy](#create-policy)
  - [Configure AppRole](#configure-approle)
  - [Generate Credentials](#generate-credentials)
- [PiSovereign Integration](#pisovereign-integration)
  - [Configuration Options](#configuration-options)
  - [ChainedSecretStore](#chainedsecretstore)
  - [Secret Paths](#secret-paths)
- [Operations](#operations)
  - [Secret Rotation](#secret-rotation)
  - [Backup and Recovery](#backup-and-recovery)
  - [Monitoring](#monitoring)

---

## Overview

HashiCorp Vault provides centralized secret management with:

- **Encryption at rest and in transit**
- **Dynamic secret generation**
- **Fine-grained access control**
- **Audit logging**
- **Secret rotation**

### Architecture

```
┌─────────────────────────────────────────────────────┐
│                    PiSovereign                       │
│  ┌─────────────────────────────────────────────┐   │
│  │           ChainedSecretStore                 │   │
│  │  ┌─────────────┐    ┌──────────────────┐   │   │
│  │  │ VaultSecret │ →  │ EnvironmentSecret │   │   │
│  │  │   Store     │    │     Store         │   │   │
│  │  └─────────────┘    └──────────────────┘   │   │
│  └─────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
           │
           │ HTTPS (mTLS optional)
           ▼
┌─────────────────────────────────────────────────────┐
│                 HashiCorp Vault                      │
│  ┌──────────────┐  ┌─────────────┐  ┌───────────┐ │
│  │ KV v2 Engine │  │   AppRole   │  │  Audit    │ │
│  │              │  │    Auth     │  │   Log     │ │
│  └──────────────┘  └─────────────┘  └───────────┘ │
└─────────────────────────────────────────────────────┘
```

---

## Installation

### Docker Installation

The simplest way to run Vault for development or small deployments:

```bash
# Create data directory
sudo mkdir -p /opt/vault/data
sudo chown 1000:1000 /opt/vault/data

# Create configuration
sudo mkdir -p /opt/vault/config
sudo nano /opt/vault/config/vault.hcl
```

Add configuration:

```hcl
# /opt/vault/config/vault.hcl

ui = true
disable_mlock = true

storage "file" {
  path = "/vault/data"
}

listener "tcp" {
  address     = "0.0.0.0:8200"
  tls_disable = false
  tls_cert_file = "/vault/certs/vault.crt"
  tls_key_file  = "/vault/certs/vault.key"
}

api_addr = "https://vault.local:8200"
cluster_addr = "https://vault.local:8201"
```

For development without TLS (not for production):

```hcl
listener "tcp" {
  address     = "0.0.0.0:8200"
  tls_disable = true
}
```

Create Docker Compose file:

```bash
sudo nano /opt/vault/docker-compose.yml
```

```yaml
version: '3.8'

services:
  vault:
    image: hashicorp/vault:1.15
    container_name: vault
    restart: unless-stopped
    ports:
      - "8200:8200"
    environment:
      VAULT_ADDR: 'http://127.0.0.1:8200'
      VAULT_API_ADDR: 'http://127.0.0.1:8200'
    cap_add:
      - IPC_LOCK
    volumes:
      - /opt/vault/data:/vault/data
      - /opt/vault/config:/vault/config
      - /opt/vault/certs:/vault/certs:ro
    command: server -config=/vault/config/vault.hcl

volumes:
  vault_data:
```

Start Vault:

```bash
cd /opt/vault
sudo docker compose up -d

# Check logs
sudo docker compose logs -f vault
```

### Native Installation

For Raspberry Pi OS:

```bash
# Add HashiCorp GPG key
curl -fsSL https://apt.releases.hashicorp.com/gpg | sudo gpg --dearmor -o /usr/share/keyrings/hashicorp.gpg

# Add repository
echo "deb [arch=arm64 signed-by=/usr/share/keyrings/hashicorp.gpg] https://apt.releases.hashicorp.com bookworm main" | \
  sudo tee /etc/apt/sources.list.d/hashicorp.list

# Install Vault
sudo apt update
sudo apt install -y vault

# Verify installation
vault version
```

Create systemd service:

```bash
# Create directories
sudo mkdir -p /opt/vault/data
sudo chown vault:vault /opt/vault/data

# Create configuration
sudo nano /etc/vault.d/vault.hcl
```

```hcl
ui = true
disable_mlock = true

storage "file" {
  path = "/opt/vault/data"
}

listener "tcp" {
  address     = "127.0.0.1:8200"
  tls_disable = true  # Enable TLS in production
}

api_addr = "http://127.0.0.1:8200"
```

Enable and start:

```bash
sudo systemctl enable vault
sudo systemctl start vault
sudo systemctl status vault
```

---

## Configuration

### Initialize Vault

Set the Vault address:

```bash
export VAULT_ADDR='http://127.0.0.1:8200'
```

Initialize with key shares:

```bash
# Initialize with 5 key shares, 3 required to unseal
vault operator init -key-shares=5 -key-threshold=3

# Output (SAVE THESE SECURELY):
# Unseal Key 1: xxxxx
# Unseal Key 2: xxxxx
# Unseal Key 3: xxxxx
# Unseal Key 4: xxxxx
# Unseal Key 5: xxxxx
# Initial Root Token: hvs.xxxxx
```

> ⚠️ **Critical**: Store unseal keys and root token securely. Distribute keys to different trusted individuals. Loss of keys = loss of access to secrets.

For development (single key):

```bash
vault operator init -key-shares=1 -key-threshold=1
```

### Unseal Process

Vault starts in a sealed state. You need to unseal it after every restart:

```bash
# Unseal with threshold keys (3 of 5)
vault operator unseal <unseal-key-1>
vault operator unseal <unseal-key-2>
vault operator unseal <unseal-key-3>

# Check status
vault status
```

Output shows:

```
Key             Value
---             -----
Seal Type       shamir
Initialized     true
Sealed          false
...
```

### Enable KV Secrets Engine

Login with root token:

```bash
vault login <root-token>
```

Enable KV v2 secrets engine:

```bash
# Enable at default path 'secret/'
vault secrets enable -path=secret -version=2 kv

# Verify
vault secrets list
```

---

## AppRole Authentication

AppRole is the recommended authentication method for machine-to-machine access like PiSovereign.

### Create Policy

Create a policy file:

```bash
nano pisovereign-policy.hcl
```

```hcl
# PiSovereign Secrets Policy

# Read secrets at secret/pisovereign/*
path "secret/data/pisovereign/*" {
  capabilities = ["read"]
}

# List secrets (for discovery)
path "secret/metadata/pisovereign/*" {
  capabilities = ["list"]
}

# Allow token renewal
path "auth/token/renew-self" {
  capabilities = ["update"]
}
```

Write the policy:

```bash
vault policy write pisovereign pisovereign-policy.hcl

# Verify
vault policy read pisovereign
```

### Configure AppRole

Enable AppRole auth method:

```bash
vault auth enable approle
```

Create the role:

```bash
vault write auth/approle/role/pisovereign \
    token_policies="pisovereign" \
    token_ttl=1h \
    token_max_ttl=4h \
    secret_id_ttl=720h \
    secret_id_num_uses=0
```

Parameters explained:

| Parameter | Value | Description |
|-----------|-------|-------------|
| `token_policies` | `pisovereign` | Policy to attach to tokens |
| `token_ttl` | `1h` | Token lifetime |
| `token_max_ttl` | `4h` | Maximum token renewal lifetime |
| `secret_id_ttl` | `720h` | Secret ID valid for 30 days |
| `secret_id_num_uses` | `0` | Unlimited uses |

### Generate Credentials

Get Role ID (static identifier):

```bash
vault read auth/approle/role/pisovereign/role-id

# Output:
# role_id    12345678-1234-1234-1234-123456789012
```

Generate Secret ID (like a password):

```bash
vault write -f auth/approle/role/pisovereign/secret-id

# Output:
# secret_id             abcd1234-abcd-1234-abcd-abcd12345678
# secret_id_accessor    accessor-id
```

Test authentication:

```bash
vault write auth/approle/login \
    role_id="12345678-1234-1234-1234-123456789012" \
    secret_id="abcd1234-abcd-1234-abcd-abcd12345678"

# Returns a client token
```

---

## PiSovereign Integration

### Store Secrets in Vault

Create the secrets PiSovereign needs:

```bash
# WhatsApp credentials
vault kv put secret/pisovereign/whatsapp \
    access_token="your-meta-access-token" \
    app_secret="your-app-secret"

# Proton Mail credentials
vault kv put secret/pisovereign/proton \
    password="your-bridge-password"

# CalDAV credentials
vault kv put secret/pisovereign/caldav \
    username="your-username" \
    password="your-password"

# OpenAI API key (for speech fallback)
vault kv put secret/pisovereign/openai \
    api_key="sk-your-openai-key"

# API keys for HTTP authentication
vault kv put secret/pisovereign/api_keys \
    key1="user-uuid-1" \
    key2="user-uuid-2"

# Verify
vault kv get secret/pisovereign/whatsapp
```

### Configuration Options

Add Vault configuration to `/etc/pisovereign/config.toml`:

```toml
# ======================
# Vault Secret Store
# ======================
[vault]
# Vault server address
address = "http://127.0.0.1:8200"

# Authentication: AppRole (recommended)
role_id = "12345678-1234-1234-1234-123456789012"
secret_id = "abcd1234-abcd-1234-abcd-abcd12345678"

# Or: Token authentication (for development)
# token = "hvs.your-token"

# KV engine mount path
mount_path = "secret"

# Request timeout
timeout_secs = 5

# Vault Enterprise namespace (optional)
# namespace = "admin/pisovereign"
```

> 💡 **Best Practice**: Store `secret_id` as an environment variable rather than in config:
>
> ```bash
> export PISOVEREIGN_VAULT_SECRET_ID="abcd1234-..."
> ```

### ChainedSecretStore

PiSovereign's `ChainedSecretStore` tries multiple backends in order:

1. **Vault** (primary) - Production secrets
2. **Environment Variables** (fallback) - Development/override

Configuration:

```toml
[secrets]
# Enable chained secret store
chain_enabled = true

# Vault is primary (see [vault] section)
# Environment variables as fallback

# Environment variable prefix (optional)
env_prefix = "PISOVEREIGN"
```

With this configuration:
- Secret `whatsapp/access_token` is looked up as:
  1. `secret/pisovereign/whatsapp` → `access_token` in Vault
  2. `PISOVEREIGN_WHATSAPP_ACCESS_TOKEN` environment variable

### Secret Paths

PiSovereign expects secrets at these paths:

| Secret | Vault Path | Environment Variable |
|--------|------------|---------------------|
| WhatsApp Access Token | `secret/pisovereign/whatsapp` → `access_token` | `PISOVEREIGN_WHATSAPP_ACCESS_TOKEN` |
| WhatsApp App Secret | `secret/pisovereign/whatsapp` → `app_secret` | `PISOVEREIGN_WHATSAPP_APP_SECRET` |
| Proton Password | `secret/pisovereign/proton` → `password` | `PISOVEREIGN_PROTON_PASSWORD` |
| CalDAV Username | `secret/pisovereign/caldav` → `username` | `PISOVEREIGN_CALDAV_USERNAME` |
| CalDAV Password | `secret/pisovereign/caldav` → `password` | `PISOVEREIGN_CALDAV_PASSWORD` |
| OpenAI API Key | `secret/pisovereign/openai` → `api_key` | `PISOVEREIGN_OPENAI_API_KEY` |

---

## Operations

### Secret Rotation

Rotate a secret without downtime:

```bash
# Update secret (creates new version)
vault kv put secret/pisovereign/whatsapp \
    access_token="new-access-token" \
    app_secret="same-app-secret"

# PiSovereign reads latest version automatically
```

View secret versions:

```bash
vault kv metadata get secret/pisovereign/whatsapp
```

Rollback if needed:

```bash
# Rollback to version 2
vault kv rollback -version=2 secret/pisovereign/whatsapp
```

### Rotate AppRole Secret ID

Periodically rotate the Secret ID:

```bash
# Generate new Secret ID
vault write -f auth/approle/role/pisovereign/secret-id

# Update PiSovereign configuration
sudo systemctl restart pisovereign
```

Automate with a cron job:

```bash
#!/bin/bash
# /usr/local/bin/rotate-vault-secret.sh

NEW_SECRET=$(vault write -field=secret_id -f auth/approle/role/pisovereign/secret-id)
echo "PISOVEREIGN_VAULT_SECRET_ID=$NEW_SECRET" > /etc/pisovereign/vault-secret.env
systemctl restart pisovereign
```

### Backup and Recovery

Backup Vault data:

```bash
# For file storage backend
sudo tar -czf vault-backup-$(date +%Y%m%d).tar.gz /opt/vault/data
```

For disaster recovery, ensure you have:
- Unseal keys (stored separately, securely)
- Root token (or ability to generate new one)
- Configuration files
- Policy definitions

### Monitoring

Monitor Vault health:

```bash
# Health endpoint
curl http://127.0.0.1:8200/v1/sys/health

# Audit logs (if enabled)
vault audit enable file file_path=/var/log/vault/audit.log
```

Vault metrics for Prometheus:

```bash
# Enable Prometheus metrics
vault write sys/config/telemetry prometheus_retention_time=60s
```

Add to Prometheus config:

```yaml
scrape_configs:
  - job_name: 'vault'
    metrics_path: '/v1/sys/metrics'
    params:
      format: ['prometheus']
    static_configs:
      - targets: ['vault.local:8200']
```

---

## Troubleshooting

### Cannot connect to Vault

```bash
# Check Vault is running
systemctl status vault

# Check network connectivity
curl -v http://127.0.0.1:8200/v1/sys/health
```

### Permission denied

```bash
# Verify token has correct policy
vault token lookup

# Check policy allows access
vault policy read pisovereign
```

### Secret not found

```bash
# Verify secret exists
vault kv get secret/pisovereign/whatsapp

# Check mount path matches config
vault secrets list
```

### Token expired

PiSovereign automatically renews tokens. If renewal fails:

```bash
# Generate new Secret ID
vault write -f auth/approle/role/pisovereign/secret-id

# Update and restart
sudo systemctl restart pisovereign
```

---

## Next Steps

- [Configuration Reference](./configuration.md) - All PiSovereign options
- [Security Hardening](../security/hardening.md) - Advanced Vault security
- [Production Deployment](../operations/deployment.md) - Deploy with TLS
