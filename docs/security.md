# Security Guide

This document covers security best practices and configuration for PiSovereign deployments.

## Authentication

### API Key Management

PiSovereign uses API key authentication with Argon2id hashing for secure storage.

#### Generating Hashed API Keys

**Never store plaintext API keys in configuration files.** Use the CLI tool to generate secure hashes:

```bash
# Generate a hashed API key
./target/release/pisovereign-cli hash-api-key "your-secret-api-key"
# Output: $argon2id$v=19$m=19456,t=2,p=1$[salt]$[hash]

# Verify a key against a hash
./target/release/pisovereign-cli hash-api-key "your-secret-api-key" --verify '$argon2id$v=19$m=19456,t=2,p=1$[salt]$[hash]'
```

#### Configuration

Add hashed keys to `config.toml`:

```toml
[security]
api_keys = [
    "$argon2id$v=19$m=19456,t=2,p=1$randomsalt1$hashedkey1",
    "$argon2id$v=19$m=19456,t=2,p=1$randomsalt2$hashedkey2",
]
```

#### Multi-User Setup

Map API keys to user IDs for per-user data isolation:

```toml
[security.user_mapping]
"$argon2id$v=19$..." = "alice"
"$argon2id$v=19$..." = "bob"
```

### Argon2 Parameters

PiSovereign uses secure Argon2id parameters:

| Parameter | Value | Description |
|-----------|-------|-------------|
| Memory | 19 MiB | Resistant to GPU attacks |
| Iterations | 2 | Balances security and performance |
| Parallelism | 1 | Suitable for server-side verification |
| Salt | 16 bytes | Unique per key, randomly generated |

### Security Warnings

PiSovereign logs warnings at startup for insecure configurations:

- **Plaintext API keys**: Warns if any configured API keys are not Argon2 hashes
- **Permissive CORS**: Warns if CORS allows all origins (`*`)
- **Insecure TLS**: Warns if Proton Bridge TLS verification is disabled

Check logs after startup and address any warnings before production deployment.

## Network Security

### TLS/HTTPS

PiSovereign does not include built-in TLS. Use a reverse proxy:

#### Caddy (Recommended)

```caddyfile
your-domain.com {
    reverse_proxy localhost:3000
}
```

#### nginx

```nginx
server {
    listen 443 ssl http2;
    server_name your-domain.com;

    ssl_certificate /etc/ssl/certs/cert.pem;
    ssl_certificate_key /etc/ssl/private/key.pem;

    location / {
        proxy_pass http://localhost:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

### CORS Configuration

Configure CORS appropriately for your deployment:

```toml
[server.cors]
# Development (permissive - NOT for production)
allowed_origins = ["*"]

# Production (restrictive)
allowed_origins = [
    "https://your-app.com",
    "https://your-admin.com"
]
```

### Firewall Rules

Recommended iptables/nftables rules:

```bash
# Allow SSH (restrict to your IP)
sudo iptables -A INPUT -p tcp --dport 22 -s YOUR_IP -j ACCEPT

# Allow HTTPS
sudo iptables -A INPUT -p tcp --dport 443 -j ACCEPT

# Allow internal Prometheus scraping
sudo iptables -A INPUT -p tcp --dport 9090 -s 10.0.0.0/8 -j ACCEPT

# Block direct HTTP access (use reverse proxy)
sudo iptables -A INPUT -p tcp --dport 3000 -s 127.0.0.1 -j ACCEPT
sudo iptables -A INPUT -p tcp --dport 3000 -j DROP
```

## Data Protection

### Database Security

SQLite database contains:
- Conversation history
- User profiles
- Email drafts
- Approval queue

Protect with appropriate permissions:

```bash
chmod 600 pisovereign.db
chown pisovereign:pisovereign pisovereign.db
```

### Encryption at Rest

For sensitive deployments, use encrypted storage:

```bash
# Create encrypted volume
sudo cryptsetup luksFormat /dev/sda1
sudo cryptsetup open /dev/sda1 secure_data
sudo mkfs.ext4 /dev/mapper/secure_data
sudo mount /dev/mapper/secure_data /data
```

### Log Security

Logs may contain sensitive information. Configure appropriately:

```toml
[logging]
# Don't log request bodies in production
log_body = false

# Mask sensitive headers
mask_headers = ["Authorization", "X-Api-Key"]
```

## Proton Bridge Integration

### TLS Configuration

**Always use TLS verification in production:**

```toml
[proton]
# Production (secure)
tls_config = "verify"

# Development only (insecure - local bridge)
# tls_config = "insecure"  # Shows warning at startup
```

### Bridge Security

When using Proton Bridge:

1. Run bridge on localhost only
2. Use strong bridge password
3. Don't expose bridge ports to network
4. Keep bridge updated

## Secrets Management

### Environment Variables

For production, use environment variables for sensitive values:

```bash
export PISOVEREIGN_API_KEY_HASH="$argon2id$..."
export PROTON_BRIDGE_PASSWORD="..."
```

### HashiCorp Vault Integration

PiSovereign supports Vault for secrets:

```toml
[secrets]
backend = "vault"
vault_addr = "https://vault.your-domain.com"
vault_token_path = "/run/secrets/vault-token"
```

## Rate Limiting

Configure rate limits to prevent abuse:

```toml
[security.rate_limit]
# Requests per minute per API key
requests_per_minute = 60

# Burst allowance
burst_size = 10

# Inference-specific limits (more restrictive)
inference_per_minute = 20
```

## Audit Logging

Enable audit logging for compliance:

```toml
[logging]
audit = true
audit_path = "/var/log/pisovereign/audit.log"
```

Audit logs include:
- All API requests with user ID
- Authentication attempts (success/failure)
- Configuration changes
- Approval workflow actions

## Security Checklist

Before production deployment:

### Authentication
- [ ] All API keys are Argon2 hashed
- [ ] Multi-user mapping configured if needed
- [ ] No default/test keys in config

### Network
- [ ] TLS configured via reverse proxy
- [ ] CORS restricted to known origins
- [ ] Firewall rules applied
- [ ] Internal services not exposed

### Data
- [ ] Database file permissions set (600)
- [ ] Logs don't contain sensitive data
- [ ] Backups encrypted

### Monitoring
- [ ] Failed auth attempts monitored
- [ ] Rate limit violations alerted
- [ ] Security warnings addressed

### Updates
- [ ] Dependency audit clean (`cargo audit`)
- [ ] No known CVEs in dependencies
- [ ] Update process documented

## Incident Response

If a security incident occurs:

1. **Rotate all API keys** immediately
2. **Check audit logs** for unauthorized access
3. **Review database** for unauthorized changes
4. **Update configurations** to address vulnerability
5. **Notify affected users** if data was exposed

## See Also

- [Deployment Guide](deployment.md)
- [Hardware Setup Guide](hardware-setup.md)
- [Rust Security Best Practices](https://anssi-fr.github.io/rust-guide/)
