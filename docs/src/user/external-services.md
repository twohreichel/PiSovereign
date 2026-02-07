# External Services Setup

> 🔗 Configure WhatsApp, Proton Mail, CalDAV, and OpenAI integrations

This guide covers setting up all external services that integrate with PiSovereign.

## Table of Contents

- [WhatsApp Business](#whatsapp-business)
  - [Meta Business Account](#meta-business-account)
  - [WhatsApp App Setup](#whatsapp-app-setup)
  - [Webhook Configuration](#webhook-configuration)
  - [PiSovereign Configuration](#pisovereign-whatsapp-configuration)
- [Proton Mail Bridge](#proton-mail-bridge)
  - [Bridge Installation](#bridge-installation)
  - [Bridge Configuration](#bridge-configuration)
  - [PiSovereign Configuration](#pisovereign-proton-configuration)
- [CalDAV (Baïkal)](#caldav-baïkal)
  - [Baïkal Installation](#baïkal-installation)
  - [Calendar Setup](#calendar-setup)
  - [PiSovereign Configuration](#pisovereign-caldav-configuration)
- [OpenAI API](#openai-api)
  - [Account Setup](#openai-account-setup)
  - [API Key Generation](#api-key-generation)
  - [PiSovereign Configuration](#pisovereign-openai-configuration)

---

## WhatsApp Business

PiSovereign uses the WhatsApp Business API for bidirectional messaging.

### Meta Business Account

1. **Create Meta Business Account**
   - Go to [business.facebook.com](https://business.facebook.com)
   - Click "Create Account"
   - Complete business verification

2. **Create Meta Developer Account**
   - Go to [developers.facebook.com](https://developers.facebook.com)
   - Click "Get Started"
   - Link to your Business Account

### WhatsApp App Setup

1. **Create App**
   - Go to [developers.facebook.com/apps](https://developers.facebook.com/apps)
   - Click "Create App"
   - Select "Business" type
   - Name: "PiSovereign" (or your preference)
   - Select your Business Account

2. **Add WhatsApp Product**
   - In App Dashboard, click "Add Products"
   - Find "WhatsApp" and click "Set up"
   - Accept terms

3. **Get Test Number**
   - Go to WhatsApp > Getting Started
   - Note the temporary test phone number
   - Note the Phone Number ID
   - Add your personal number to allowed recipients

4. **Generate Access Token**
   - In WhatsApp > Getting Started
   - Click "Generate Access Token"
   - Copy the temporary token (valid 24h)
   
   For permanent token:
   - Go to Business Settings > System Users
   - Create System User (Admin)
   - Generate token with `whatsapp_business_messaging` permission

5. **Note Required Values**
   
   | Value | Where to Find |
   |-------|---------------|
   | `access_token` | Generated above |
   | `phone_number_id` | WhatsApp > Getting Started |
   | `app_secret` | App Settings > Basic |
   | `verify_token` | You create this (any string) |

### Webhook Configuration

PiSovereign needs a public URL for webhooks. Options:

**Option A: Traefik (Production)**

See [Deployment Guide](../operations/deployment.md) for full Traefik setup.

```bash
# Your webhook URL will be:
https://your-domain.com/v1/webhooks/whatsapp
```

**Option B: ngrok (Development)**

```bash
# Install ngrok
curl -s https://ngrok-agent.s3.amazonaws.com/ngrok.asc | \
  sudo tee /etc/apt/trusted.gpg.d/ngrok.asc >/dev/null && \
  echo "deb https://ngrok-agent.s3.amazonaws.com buster main" | \
  sudo tee /etc/apt/sources.list.d/ngrok.list && \
  sudo apt update && sudo apt install ngrok

# Start tunnel
ngrok http 3000
```

**Configure Webhook in Meta:**

1. Go to WhatsApp > Configuration
2. Click "Edit" on Webhooks
3. Enter:
   - Callback URL: `https://your-domain.com/v1/webhooks/whatsapp`
   - Verify Token: Your chosen verify_token
4. Click "Verify and Save"
5. Subscribe to: `messages`, `message_template_status_update`

### PiSovereign WhatsApp Configuration

Store sensitive values in Vault:

```bash
vault kv put secret/pisovereign/whatsapp \
    access_token="your-access-token" \
    app_secret="your-app-secret"
```

Add to `config.toml`:

```toml
[whatsapp]
# From Meta Developer Portal
phone_number_id = "your-phone-number-id"
verify_token = "your-verify-token"

# Require webhook signature verification (recommended)
signature_required = true

# API version
api_version = "v18.0"
```

Test the integration:

```bash
# Send a test message
curl -X POST http://localhost:3000/v1/whatsapp/send \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-api-key" \
  -d '{
    "to": "+1234567890",
    "message": "Hello from PiSovereign!"
  }'
```

---

## Proton Mail Bridge

Proton Mail Bridge provides local IMAP/SMTP access to your Proton Mail account.

### Bridge Installation

**On Raspberry Pi:**

```bash
# Download Bridge for ARM64
wget https://proton.me/download/bridge/protonmail-bridge_3.7.1-1_arm64.deb

# Install
sudo dpkg -i protonmail-bridge_3.7.1-1_arm64.deb
sudo apt install -f  # Fix dependencies if needed
```

**On Desktop (for initial setup):**

```bash
# Ubuntu/Debian
wget https://proton.me/download/bridge/protonmail-bridge_3.7.1-1_amd64.deb
sudo dpkg -i protonmail-bridge_3.7.1-1_amd64.deb

# macOS
brew install --cask protonmail-bridge
```

### Bridge Configuration

1. **Start Bridge (GUI for initial setup)**
   ```bash
   protonmail-bridge
   ```

2. **Sign In**
   - Click "Sign In"
   - Enter Proton credentials
   - Complete 2FA if enabled

3. **Get Bridge Password**
   - Click on your account
   - Note the "Bridge Password" (NOT your Proton password)
   - This is what PiSovereign uses

4. **Note Connection Details**
   
   | Setting | Default Value |
   |---------|---------------|
   | IMAP Host | `127.0.0.1` |
   | IMAP Port | `1143` |
   | SMTP Host | `127.0.0.1` |
   | SMTP Port | `1025` |

5. **Run Bridge Headless (Production)**
   
   Create systemd service:
   ```bash
   sudo nano /etc/systemd/system/protonmail-bridge.service
   ```
   
   ```ini
   [Unit]
   Description=Proton Mail Bridge
   After=network.target
   
   [Service]
   Type=simple
   User=pi
   ExecStart=/usr/bin/protonmail-bridge --noninteractive
   Restart=always
   RestartSec=10
   
   [Install]
   WantedBy=multi-user.target
   ```
   
   ```bash
   sudo systemctl enable protonmail-bridge
   sudo systemctl start protonmail-bridge
   ```

### PiSovereign Proton Configuration

Store password in Vault:

```bash
vault kv put secret/pisovereign/proton \
    password="your-bridge-password"
```

Add to `config.toml`:

```toml
[proton]
# Bridge connection (default localhost)
imap_host = "127.0.0.1"
imap_port = 1143
smtp_host = "127.0.0.1"
smtp_port = 1025

# Your Proton email address
email = "yourname@proton.me"

# TLS settings (Bridge uses self-signed certs)
[proton.tls]
verify_certificates = false
min_tls_version = "1.2"
```

Test the integration:

```bash
# Check email status
pisovereign-cli status --service email

# Read recent emails
pisovereign-cli command "check emails"
```

---

## CalDAV (Baïkal)

Baïkal is a lightweight, self-hosted CalDAV server perfect for Raspberry Pi.

### Baïkal Installation

```bash
# Install dependencies
sudo apt install -y nginx php-fpm php-sqlite3 php-mbstring php-xml

# Download Baïkal
cd /var/www
sudo wget https://github.com/sabre-io/Baikal/releases/download/0.9.4/baikal-0.9.4.zip
sudo unzip baikal-0.9.4.zip
sudo mv baikal /var/www/baikal
sudo chown -R www-data:www-data /var/www/baikal
```

Configure Nginx:

```bash
sudo nano /etc/nginx/sites-available/baikal
```

```nginx
server {
    listen 8080;
    server_name localhost;
    
    root /var/www/baikal/html;
    index index.php;
    
    rewrite ^/.well-known/caldav /dav.php redirect;
    rewrite ^/.well-known/carddav /dav.php redirect;
    
    location ~ ^(.+\.php)(.*)$ {
        try_files $fastcgi_script_name =404;
        include fastcgi_params;
        fastcgi_split_path_info ^(.+\.php)(.*)$;
        fastcgi_pass unix:/run/php/php-fpm.sock;
        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
        fastcgi_param PATH_INFO $fastcgi_path_info;
    }
    
    location / {
        try_files $uri $uri/ =404;
    }
}
```

```bash
sudo ln -s /etc/nginx/sites-available/baikal /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl restart nginx
```

### Calendar Setup

1. **Initial Setup**
   - Open `http://localhost:8080` in browser
   - Complete setup wizard
   - Set admin password
   - Choose SQLite database

2. **Create User**
   - Go to Users and Resources
   - Add new user
   - Note username and password

3. **Create Calendar**
   - Users can create calendars via any CalDAV client
   - Or use the admin interface

4. **Find Calendar URL**
   ```
   http://localhost:8080/dav.php/calendars/USERNAME/default/
   ```

### PiSovereign CalDAV Configuration

Store credentials in Vault:

```bash
vault kv put secret/pisovereign/caldav \
    username="your-username" \
    password="your-password"
```

Add to `config.toml`:

```toml
[caldav]
# Baïkal server URL
server_url = "http://localhost:8080/dav.php"

# Default calendar path
calendar_path = "/calendars/username/default/"

# TLS (enable if using HTTPS)
verify_certs = true
timeout_secs = 30
```

Test the integration:

```bash
# Check calendar status
pisovereign-cli status --service calendar

# List upcoming events
pisovereign-cli command "what's on my calendar today"
```

---

## OpenAI API

OpenAI API is used for cloud-based speech processing (STT/TTS fallback).

### OpenAI Account Setup

1. **Create Account**
   - Go to [platform.openai.com](https://platform.openai.com)
   - Sign up or log in

2. **Add Payment Method**
   - Go to Settings > Billing
   - Add payment method
   - Set usage limits (recommended)

3. **Set Usage Limits**
   - Monthly budget: Start with $10-20
   - Enables automatic cutoff to prevent unexpected charges

### API Key Generation

1. **Create API Key**
   - Go to [platform.openai.com/api-keys](https://platform.openai.com/api-keys)
   - Click "Create new secret key"
   - Name: "PiSovereign"
   - Copy the key immediately (shown only once)

2. **Restrict Permissions (Optional)**
   - For the key, enable only:
     - `audio.speech` (TTS)
     - `audio.transcriptions` (STT)

### PiSovereign OpenAI Configuration

Store API key in Vault:

```bash
vault kv put secret/pisovereign/openai \
    api_key="sk-your-openai-key"
```

Add to `config.toml`:

```toml
[speech]
# Use hybrid mode: local first, OpenAI fallback
provider = "hybrid"

# OpenAI settings
openai_base_url = "https://api.openai.com/v1"
stt_model = "whisper-1"
tts_model = "tts-1"
default_voice = "nova"  # Options: alloy, echo, fable, onyx, nova, shimmer
output_format = "opus"
timeout_ms = 60000

[speech.hybrid]
prefer_local = true
allow_cloud_fallback = true
```

For local-only (maximum privacy):

```toml
[speech]
provider = "local"

[speech.hybrid]
prefer_local = true
allow_cloud_fallback = false  # Never use cloud
```

Test the integration:

```bash
# Test TTS
pisovereign-cli tts "Hello, this is a test"

# Test STT (requires audio file)
pisovereign-cli stt /path/to/audio.wav
```

---

## Integration Status Check

Verify all integrations:

```bash
# Check all services
pisovereign-cli status

# Output:
# Service          Status    Latency
# ─────────────────────────────────
# Inference        ✓ OK      45ms
# Database         ✓ OK      2ms
# WhatsApp         ✓ OK      120ms
# Email (Proton)   ✓ OK      89ms
# Calendar         ✓ OK      35ms
# Weather          ✓ OK      180ms
# Speech (Local)   ✓ OK      -
# Speech (Cloud)   ✓ OK      -
```

Check detailed health:

```bash
curl http://localhost:3000/ready/all | jq
```

---

## Troubleshooting

### WhatsApp webhook not receiving messages

1. Check webhook URL is publicly accessible
2. Verify `verify_token` matches
3. Check webhook is subscribed to `messages`
4. Review Meta App Dashboard for delivery errors

### Proton Bridge connection refused

1. Ensure Bridge is running: `systemctl status protonmail-bridge`
2. Check Bridge password (not Proton password)
3. Verify ports 1143/1025 are not blocked

### CalDAV authentication failed

1. Verify username/password
2. Check calendar path format
3. Test with curl:
   ```bash
   curl -u username:password http://localhost:8080/dav.php/calendars/username/
   ```

### OpenAI rate limited

1. Check billing status at platform.openai.com
2. Review usage limits
3. PiSovereign will automatically fall back to local if configured

---

## Next Steps

- [Configuration Reference](./configuration.md) - Fine-tune all options
- [Monitoring](../operations/monitoring.md) - Track service health
- [Security Hardening](../security/hardening.md) - Secure your integrations
