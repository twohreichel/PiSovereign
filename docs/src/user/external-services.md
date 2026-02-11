# External Services Setup

> 🔗 Configure WhatsApp, Signal, Proton Mail, CalDAV, OpenAI, and Brave Search integrations

This guide covers setting up all external services that integrate with PiSovereign.

## Table of Contents

- [Messenger Selection](#messenger-selection)
- [WhatsApp Business](#whatsapp-business)
  - [Meta Business Account](#meta-business-account)
  - [WhatsApp App Setup](#whatsapp-app-setup)
  - [Webhook Configuration](#webhook-configuration)
  - [PiSovereign Configuration](#pisovereign-whatsapp-configuration)
- [Signal Messenger](#signal-messenger)
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
- [Brave Search API](#brave-search-api)
  - [Account Setup](#brave-account-setup)
  - [API Key Generation](#brave-api-key-generation)
  - [PiSovereign Configuration](#pisovereign-websearch-configuration)
  - [DuckDuckGo Fallback](#duckduckgo-fallback)

---

## Messenger Selection

PiSovereign supports one messenger at a time. Configure which messenger to use:

```toml
# In config.toml - choose one:
messenger = "whatsapp"   # Use WhatsApp Business API
messenger = "signal"     # Use Signal via signal-cli
messenger = "none"       # Disable messenger integration
```

| Messenger | Use Case |
|-----------|----------|
| WhatsApp | Business integration, webhook-based, requires public URL |
| Signal | Privacy-focused, polling-based, no public URL needed |

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

## Signal Messenger

Signal provides a privacy-focused alternative to WhatsApp using end-to-end encryption.

### Key Features

- **No Public URL Required**: Uses polling instead of webhooks
- **End-to-End Encrypted**: All messages are encrypted
- **Personal Account**: Uses your existing Signal account
- **Voice Messages**: Supports voice message transcription

### Quick Setup

1. Install signal-cli (Java-based CLI for Signal)
2. Register your phone number with Signal
3. Start the signal-cli daemon
4. Configure PiSovereign

```toml
messenger = "signal"

[signal]
phone_number = "+1234567890"
socket_path = "/var/run/signal-cli/socket"
```

📖 **For detailed instructions, see [Signal Setup Guide](./signal-setup.md)**

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

Baïkal is a lightweight, self-hosted CalDAV server perfect for managing calendars, task lists, and todos. It supports both CalDAV (calendars/todos) and CardDAV (contacts).

### Docker Installation (Recommended)

The easiest way to deploy Baïkal is via Docker, integrated into the PiSovereign setup:

```bash
# macOS
./scripts/setup-mac.sh --baikal

# Raspberry Pi
sudo ./scripts/setup-pi.sh --baikal
```

You can also enable Baïkal interactively when running the setup script without the `--baikal` flag — the script will prompt you.

This adds a `baikal` service to `docker-compose.yml`:

- **Image**: `ckulka/baikal:nginx`
- **Host port**: `127.0.0.1:5232` (localhost only, no external access)
- **Docker internal**: `http://baikal:80/dav.php` (used by PiSovereign)
- **Volumes**: `baikal-config` and `baikal-data` for persistent storage


> **Network access**: Baïkal is bound to `127.0.0.1:5232` and is **not accessible from outside** the host machine. PiSovereign accesses it internally via the Docker network at `http://baikal:80/dav.php`. This is the most secure configuration — no CalDAV data is exposed to the network.

### Calendar Setup

After starting the Docker containers, complete the one-time setup wizard:

1. **Initial Setup**
   - Open `http://localhost:5232` in your browser
   - Complete the setup wizard
   - Set an admin password
   - Choose SQLite database (recommended)

2. **Create User**
   - Go to Users and Resources in the admin panel
   - Add a new user (use the same username/password you configured during setup)

3. **Create Calendar**
   - Users can create calendars via any CalDAV client
   - Or use the Baïkal admin interface

4. **Update config.toml**
   - Set the `calendar_path` to match your user and calendar name:
     ```toml
     calendar_path = "/calendars/username/default/"
     ```

### PiSovereign CalDAV Configuration

When using Baïkal via Docker, the setup script automatically writes the `[caldav]` section in `config.toml`:

```toml
[caldav]
# Docker-internal URL (do not change when using Baïkal via Docker)
server_url = "http://baikal:80/dav.php"
username = "your-username"
password = "your-password"

# Update this after creating your calendar in the Baïkal wizard
calendar_path = "/calendars/username/default/"

verify_certs = true
timeout_secs = 30
```

Optionally store credentials in Vault:

```bash
vault kv put secret/pisovereign/caldav \
    username="your-username" \
    password="your-password"
```

Test the integration:

```bash
# Check calendar status
pisovereign-cli status --service calendar

# List upcoming events
pisovereign-cli command "what's on my calendar today"
```

### Native Installation (Alternative)

If you prefer running Baïkal without Docker (e.g., on a Raspberry Pi with native deployment):

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

For native installation, use `http://localhost:8080/dav.php` as the `server_url` in `config.toml`.

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

## Brave Search API

Brave Search enables web search capabilities, allowing PiSovereign to search the internet and provide answers with source citations. DuckDuckGo is used as an automatic fallback if Brave Search is unavailable.

### Brave Account Setup

1. **Create Brave Account**
   - Go to [brave.com/search/api](https://brave.com/search/api/)
   - Click "Get Started" or "Sign Up"
   - Create account with email

2. **Choose Pricing Tier**
   
   | Plan | Queries/Month | Cost | Features |
   |------|---------------|------|----------|
   | Free | 2,000 | $0 | Basic web search |
   | Base | 20,000 | $5/mo | Web search + spellcheck |
   | Pro | 100,000 | $15/mo | All features |
   
   The **Free** tier is sufficient for personal use.

3. **Accept Terms of Service**
   - Review and accept the API Terms
   - Complete account verification if required

### Brave API Key Generation

1. **Access API Dashboard**
   - Go to [api.search.brave.com](https://api.search.brave.com/)
   - Log in with your Brave account
   - Navigate to "API Keys"

2. **Create API Key**
   - Click "Create API Key"
   - Name: "PiSovereign"
   - Copy the key immediately (shown only once)

3. **Test API Key**
   
   ```bash
   curl -s "https://api.search.brave.com/res/v1/web/search?q=test" \
     -H "Accept: application/json" \
     -H "X-Subscription-Token: YOUR_API_KEY" | jq '.web.results[0].title'
   ```
   
   Expected output: The title of the first search result.

### PiSovereign Websearch Configuration

Store API key in Vault (recommended):

```bash
vault kv put secret/pisovereign/websearch \
    brave_api_key="BSA-your-brave-api-key"
```

Or add directly to `config.toml` (less secure):

```toml
[websearch]
# Brave Search API key (required for primary provider)
api_key = "BSA-your-brave-api-key"

# Maximum number of results to fetch per query (default: 5)
max_results = 5

# Request timeout in seconds (default: 30)
timeout_secs = 30

# Enable DuckDuckGo fallback if Brave fails (default: true)
fallback_enabled = true

# Safe search level: "off", "moderate", or "strict" (default: "moderate")
safe_search = "moderate"

# Country code for localized results (optional)
# Examples: "US", "DE", "GB", "FR"
country = "DE"

# Language code for results (optional)
# Examples: "en", "de", "fr", "es"
language = "de"

# Rate limit: maximum requests per minute (default: 60)
rate_limit_rpm = 60

# Cache TTL in minutes for search results (default: 15)
cache_ttl_minutes = 15
```

### DuckDuckGo Fallback

DuckDuckGo's Instant Answer API is automatically used when:

- Brave Search API key is not configured
- Brave Search returns an error
- Brave Search rate limit is exceeded

**Fallback Benefits:**
- No API key required
- Free and privacy-respecting
- Provides quick answers for common queries

**Fallback Limitations:**
- Less comprehensive results
- No full web search (instant answers only)
- May not find results for complex queries

To disable fallback and use only Brave:

```toml
[websearch]
api_key = "BSA-your-brave-api-key"
fallback_enabled = false
```

### Testing Web Search

Test the web search integration:

```bash
# Test via CLI
pisovereign-cli search "current weather in Berlin"

# Test via WhatsApp
# Send: "Search the web for Rust async patterns"
# Or in German: "Suche im Internet nach Rust async patterns"
```

Expected response format:
```
Based on web search results for "Rust async patterns":

[Summary from LLM with inline citations]

Sources:
[1] Title - example.com
[2] Another Title - docs.rs
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
