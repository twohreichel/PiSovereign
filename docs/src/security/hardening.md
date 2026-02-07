# Security Hardening

> 🔒 Comprehensive security guide for production PiSovereign deployments

This guide covers security best practices for Raspberry Pi, application, and infrastructure hardening.

## Table of Contents

- [Security Overview](#security-overview)
- [Operating System Hardening](#operating-system-hardening)
  - [User Management](#user-management)
  - [SSH Hardening](#ssh-hardening)
  - [Firewall Configuration](#firewall-configuration)
  - [Fail2ban Setup](#fail2ban-setup)
  - [Kernel Hardening](#kernel-hardening)
  - [Automatic Updates](#automatic-updates)
- [Application Security](#application-security)
  - [Principle of Least Privilege](#principle-of-least-privilege)
  - [Rate Limiting](#rate-limiting)
  - [API Authentication](#api-authentication)
  - [Input Validation](#input-validation)
- [Vault Security](#vault-security)
  - [Seal/Unseal Best Practices](#sealunseal-best-practices)
  - [Token Management](#token-management)
  - [Audit Logging](#audit-logging)
- [Network Security](#network-security)
  - [TLS Configuration](#tls-configuration)
  - [Internal Network Isolation](#internal-network-isolation)
- [Monitoring & Auditing](#monitoring--auditing)
- [Incident Response](#incident-response)
- [Security Checklist](#security-checklist)

---

## Security Overview

PiSovereign security layers:

```
┌─────────────────────────────────────────────────────────────┐
│                    Network Layer                            │
│  • UFW Firewall    • Fail2ban    • TLS 1.3                 │
├─────────────────────────────────────────────────────────────┤
│                    Application Layer                        │
│  • Rate Limiting   • Authentication  • Input Validation    │
├─────────────────────────────────────────────────────────────┤
│                    Secret Management                        │
│  • HashiCorp Vault  • Encrypted Storage  • Key Rotation    │
├─────────────────────────────────────────────────────────────┤
│                    Operating System                         │
│  • Hardened SSH    • Kernel Security   • Auto Updates      │
└─────────────────────────────────────────────────────────────┘
```

### Security Principles

1. **Defense in Depth** - Multiple security layers
2. **Principle of Least Privilege** - Minimal necessary permissions
3. **Fail Secure** - Safe defaults when things go wrong
4. **Audit Everything** - Comprehensive logging

---

## Operating System Hardening

### User Management

**Create dedicated service user**:

```bash
# Create non-login user for PiSovereign
sudo useradd -r -s /usr/sbin/nologin pisovereign

# Create group for Hailo device access
sudo groupadd hailo
sudo usermod -aG hailo pisovereign

# Set up service directories
sudo mkdir -p /var/lib/pisovereign /etc/pisovereign /var/log/pisovereign
sudo chown pisovereign:pisovereign /var/lib/pisovereign /var/log/pisovereign
sudo chmod 750 /var/lib/pisovereign
sudo chmod 755 /etc/pisovereign
```

**Secure the default user**:

```bash
# Change default password immediately
passwd

# Disable root login
sudo passwd -l root

# Remove unnecessary users
sudo deluser --remove-home games
sudo deluser --remove-home news
```

### SSH Hardening

Edit `/etc/ssh/sshd_config`:

```bash
sudo nano /etc/ssh/sshd_config
```

```text
# Change default port (obscurity, but reduces noise)
Port 2222

# Protocol version
Protocol 2

# Authentication
PermitRootLogin no
PubkeyAuthentication yes
PasswordAuthentication no
PermitEmptyPasswords no
ChallengeResponseAuthentication no
UsePAM yes

# Allowed users (adjust to your username)
AllowUsers andreas

# Key exchange algorithms (secure only)
KexAlgorithms curve25519-sha256@libssh.org,diffie-hellman-group16-sha512,diffie-hellman-group18-sha512

# Ciphers (secure only)
Ciphers chacha20-poly1305@openssh.com,aes256-gcm@openssh.com,aes128-gcm@openssh.com

# MACs (secure only)
MACs hmac-sha2-512-etm@openssh.com,hmac-sha2-256-etm@openssh.com

# Connection settings
LoginGraceTime 30
MaxAuthTries 3
MaxSessions 3
MaxStartups 3:50:10

# Idle timeout
ClientAliveInterval 300
ClientAliveCountMax 2

# Disable forwarding (unless needed)
AllowTcpForwarding no
X11Forwarding no
AllowAgentForwarding no

# Logging
LogLevel VERBOSE
```

**Generate strong SSH key**:

```bash
# On your local machine
ssh-keygen -t ed25519 -a 100 -f ~/.ssh/pisovereign

# Copy to Pi
ssh-copy-id -p 2222 -i ~/.ssh/pisovereign.pub andreas@<pi-ip>
```

**Apply changes**:

```bash
sudo systemctl restart sshd
```

### Firewall Configuration

**UFW setup**:

```bash
# Install UFW
sudo apt install -y ufw

# Default policies
sudo ufw default deny incoming
sudo ufw default allow outgoing

# Allow SSH (custom port)
sudo ufw allow 2222/tcp comment 'SSH'

# Allow HTTPS (if exposing externally)
sudo ufw allow 443/tcp comment 'HTTPS'

# Allow HTTP for Let's Encrypt (temporary, can disable after cert)
sudo ufw allow 80/tcp comment 'HTTP redirect'

# Optional: Allow from specific IPs only
# sudo ufw allow from 192.168.1.0/24 to any port 2222

# Enable firewall
sudo ufw enable

# Check status
sudo ufw status verbose
```

**Advanced IPTables rules** (optional):

```bash
# Rate limit new connections
sudo iptables -A INPUT -p tcp --dport 443 -m state --state NEW -m recent --set
sudo iptables -A INPUT -p tcp --dport 443 -m state --state NEW -m recent --update --seconds 60 --hitcount 20 -j DROP
```

### Fail2ban Setup

```bash
# Install
sudo apt install -y fail2ban

# Copy default config
sudo cp /etc/fail2ban/jail.conf /etc/fail2ban/jail.local
```

**Configure** `/etc/fail2ban/jail.local`:

```ini
[DEFAULT]
bantime = 1h
findtime = 10m
maxretry = 5
backend = systemd
banaction = ufw

[sshd]
enabled = true
port = 2222
filter = sshd
maxretry = 3
bantime = 24h

# Optional: Protect PiSovereign API
[pisovereign]
enabled = true
port = 443
filter = pisovereign
logpath = /var/log/pisovereign/access.log
maxretry = 10
findtime = 1m
bantime = 1h
```

**Create PiSovereign filter** `/etc/fail2ban/filter.d/pisovereign.conf`:

```ini
[Definition]
failregex = ^.* "request_id":"[^"]*","remote_addr":"<HOST>".* "status":(401|403|429).*$
ignoreregex =
```

**Enable**:

```bash
sudo systemctl enable fail2ban
sudo systemctl start fail2ban

# Check status
sudo fail2ban-client status
sudo fail2ban-client status sshd
```

### Kernel Hardening

Edit `/etc/sysctl.conf`:

```bash
sudo nano /etc/sysctl.conf
```

```text
# Network security
net.ipv4.tcp_syncookies = 1
net.ipv4.conf.all.accept_source_route = 0
net.ipv4.conf.default.accept_source_route = 0
net.ipv4.conf.all.accept_redirects = 0
net.ipv4.conf.default.accept_redirects = 0
net.ipv4.conf.all.secure_redirects = 0
net.ipv4.conf.default.secure_redirects = 0
net.ipv4.icmp_echo_ignore_broadcasts = 1
net.ipv4.conf.all.rp_filter = 1
net.ipv4.conf.default.rp_filter = 1
net.ipv4.conf.all.send_redirects = 0
net.ipv4.conf.default.send_redirects = 0
net.ipv4.conf.all.log_martians = 1
net.ipv4.conf.default.log_martians = 1

# Disable IPv6 (if not needed)
net.ipv6.conf.all.disable_ipv6 = 1
net.ipv6.conf.default.disable_ipv6 = 1

# Memory protection
kernel.randomize_va_space = 2
kernel.kptr_restrict = 2
kernel.yama.ptrace_scope = 1

# Filesystem hardening
fs.protected_hardlinks = 1
fs.protected_symlinks = 1
fs.suid_dumpable = 0
```

**Apply**:

```bash
sudo sysctl -p
```

### Automatic Updates

```bash
# Install unattended-upgrades
sudo apt install -y unattended-upgrades apt-listchanges

# Configure
sudo dpkg-reconfigure -plow unattended-upgrades
```

Edit `/etc/apt/apt.conf.d/50unattended-upgrades`:

```text
Unattended-Upgrade::Allowed-Origins {
    "${distro_id}:${distro_codename}";
    "${distro_id}:${distro_codename}-security";
    "${distro_id}ESMApps:${distro_codename}-apps-security";
    "${distro_id}ESM:${distro_codename}-infra-security";
};

Unattended-Upgrade::Remove-Unused-Kernel-Packages "true";
Unattended-Upgrade::Remove-New-Unused-Dependencies "true";
Unattended-Upgrade::Remove-Unused-Dependencies "true";
Unattended-Upgrade::Automatic-Reboot "false";
Unattended-Upgrade::Automatic-Reboot-Time "03:00";
```

---

## Application Security

### Principle of Least Privilege

**Systemd hardening** in `/etc/systemd/system/pisovereign.service`:

```ini
[Service]
# Run as unprivileged user
User=pisovereign
Group=pisovereign

# Restrict capabilities
CapabilityBoundingSet=
AmbientCapabilities=
NoNewPrivileges=yes

# Filesystem restrictions
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/var/lib/pisovereign /var/log/pisovereign
PrivateTmp=yes
PrivateDevices=yes

# Network restrictions
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX

# System call filtering
SystemCallFilter=@system-service
SystemCallFilter=~@privileged @resources
SystemCallErrorNumber=EPERM

# Additional hardening
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectKernelLogs=yes
ProtectControlGroups=yes
ProtectClock=yes
RestrictRealtime=yes
RestrictSUIDSGID=yes
LockPersonality=yes
MemoryDenyWriteExecute=yes

# Resource limits
MemoryMax=1G
TasksMax=100
```

### Rate Limiting

Configure in `config.toml`:

```toml
[security]
rate_limit_enabled = true
rate_limit_rpm = 60  # Requests per minute per IP

[api]
max_request_size_bytes = 1048576  # 1 MB
request_timeout_secs = 30
```

### API Authentication

**Generate secure API keys**:

```bash
# Generate 256-bit API key
openssl rand -base64 32
```

**Store in Vault**:

```bash
vault kv put secret/pisovereign/api-keys \
  admin="$(openssl rand -base64 32)" \
  readonly="$(openssl rand -base64 32)"
```

**Authentication flow**:

1. Client sends `Authorization: Bearer <api-key>` header
2. Server validates against Vault-stored keys
3. Invalid keys return 401 with generic message
4. Rate limiting applied per-key

### Input Validation

PiSovereign validates all inputs:

- **Maximum lengths** enforced on all string fields
- **Content type** verification
- **JSON schema** validation
- **Path traversal** protection
- **SQL injection** prevention via parameterized queries

---

## Vault Security

### Seal/Unseal Best Practices

**Never store unseal keys on the Pi**:

```bash
# Good: Keys stored separately, manual unseal
vault operator unseal  # Enter key interactively

# Bad: Keys stored in file on the same system
```

**Key splitting** (Shamir's Secret Sharing):

```bash
# Initialize with 5 key shares, 3 required to unseal
vault operator init -key-shares=5 -key-threshold=3

# Distribute shares to different people/locations
# Share 1 → Person A
# Share 2 → Person B
# Share 3 → Secure storage location
# etc.
```

**Auto-unseal** (for unattended systems):

```hcl
# config.hcl (using cloud KMS)
seal "awskms" {
  region     = "eu-central-1"
  kms_key_id = "alias/vault-unseal"
}
```

### Token Management

**Use AppRole for applications**:

```bash
# Create policy
vault policy write pisovereign - <<EOF
path "secret/data/pisovereign/*" {
  capabilities = ["read", "list"]
}
EOF

# Create AppRole
vault auth enable approle
vault write auth/approle/role/pisovereign \
  token_policies="pisovereign" \
  token_ttl=1h \
  token_max_ttl=4h \
  secret_id_ttl=24h

# Get credentials
vault read auth/approle/role/pisovereign/role-id
vault write -f auth/approle/role/pisovereign/secret-id
```

**Token best practices**:

- Use short TTLs (1 hour)
- Rotate secret IDs regularly
- Never log tokens
- Revoke tokens on application shutdown

### Audit Logging

**Enable audit backend**:

```bash
vault audit enable file file_path=/var/log/vault/audit.log

# Or with syslog
vault audit enable syslog
```

**Protect audit logs**:

```bash
sudo chmod 600 /var/log/vault/audit.log
sudo chown vault:vault /var/log/vault/audit.log
```

---

## Network Security

### TLS Configuration

**Minimum TLS 1.3**:

```toml
# config.toml
[security]
min_tls_version = "1.3"
tls_verify_certs = true
```

**Traefik TLS hardening**:

```yaml
# traefik/dynamic/tls.yml
tls:
  options:
    default:
      minVersion: VersionTLS13
      cipherSuites:
        - TLS_AES_256_GCM_SHA384
        - TLS_CHACHA20_POLY1305_SHA256
      curvePreferences:
        - X25519
        - CurveP384
      sniStrict: true
```

### Internal Network Isolation

**Docker network isolation**:

```yaml
# docker-compose.yml
networks:
  frontend:
    driver: bridge
  backend:
    driver: bridge
    internal: true  # No external access

services:
  traefik:
    networks:
      - frontend
      - backend

  pisovereign:
    networks:
      - backend  # Not directly accessible
```

---

## Monitoring & Auditing

### Security Monitoring

**Log critical events**:

```toml
# config.toml
[logging]
level = "info"
format = "json"
include_request_id = true
include_user_id = true
```

**Monitor for**:
- Failed authentication attempts
- Rate limit hits
- Unusual request patterns
- Vault access failures

**Prometheus alerts**:

```yaml
# prometheus/rules/security.yml
groups:
  - name: security
    rules:
      - alert: HighFailedAuthRate
        expr: rate(http_requests_client_error_total{status="401"}[5m]) > 10
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "High authentication failure rate"

      - alert: RateLimitTriggered
        expr: rate(http_requests_client_error_total{status="429"}[5m]) > 5
        for: 5m
        labels:
          severity: info
        annotations:
          summary: "Rate limiting triggered"
```

### Audit Trail

PiSovereign maintains audit logs for:

- User actions (conversations, approvals)
- Configuration changes
- Authentication events
- API access

```sql
-- Query audit logs
SELECT * FROM audit_log 
WHERE created_at > datetime('now', '-1 day')
ORDER BY created_at DESC;
```

---

## Incident Response

### Suspected Compromise

1. **Isolate** - Disconnect from network if active threat

```bash
sudo ufw default deny incoming
sudo ufw default deny outgoing
```

2. **Preserve evidence**

```bash
# Copy logs
sudo cp -r /var/log /backup/incident-$(date +%Y%m%d)/
# Memory dump if needed
```

3. **Rotate credentials**

```bash
# Regenerate all API keys
vault kv put secret/pisovereign/api-keys \
  admin="$(openssl rand -base64 32)"

# Rotate Vault tokens
vault token revoke -self
```

4. **Review access**

```bash
# Check SSH access
sudo lastlog
sudo last -50

# Check Vault audit
sudo grep -i "error\|denied" /var/log/vault/audit.log
```

5. **Restore from known-good backup**

---

## Security Checklist

### Initial Setup

- [ ] Changed default passwords
- [ ] Created dedicated service user
- [ ] SSH key-only authentication
- [ ] SSH on non-standard port
- [ ] UFW firewall enabled
- [ ] Fail2ban configured
- [ ] Kernel hardening applied
- [ ] Automatic updates enabled

### Application

- [ ] Rate limiting enabled
- [ ] API keys stored in Vault
- [ ] TLS 1.3 minimum
- [ ] Input validation active
- [ ] Systemd hardening applied
- [ ] Logs don't contain secrets

### Vault

- [ ] Unseal keys distributed/secured
- [ ] AppRole configured for PiSovereign
- [ ] Short token TTLs
- [ ] Audit logging enabled

### Ongoing

- [ ] Weekly security updates reviewed
- [ ] Monthly credential rotation
- [ ] Quarterly penetration testing
- [ ] Annual security audit

---

## References

- [CIS Raspberry Pi OS Benchmark](https://www.cisecurity.org/benchmark/raspberry_pi)
- [OWASP API Security Top 10](https://owasp.org/www-project-api-security/)
- [HashiCorp Vault Security Model](https://developer.hashicorp.com/vault/docs/internals/security)
- [Mozilla SSL Configuration Generator](https://ssl-config.mozilla.org/)
