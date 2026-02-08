# Signal Messenger Setup

> 📱 Configure Signal messenger integration using signal-cli

PiSovereign supports Signal as an alternative to WhatsApp for bidirectional messaging. This guide covers setting up Signal messenger integration using signal-cli.

## Overview

Signal integration uses [signal-cli](https://github.com/AsamK/signal-cli), a command-line interface for Signal. Unlike WhatsApp which uses webhooks, Signal messages are received by polling the signal-cli daemon.

### Key Differences from WhatsApp

| Feature | WhatsApp | Signal |
|---------|----------|--------|
| Message Delivery | Webhook (push) | Polling (pull) |
| Account Type | Business API | Personal account |
| External URL | Required (webhooks) | Not required |
| Privacy | Meta servers | End-to-end encrypted |
| Cost | Free tier limited | Free |

## Prerequisites

- **Java Runtime**: OpenJDK 17 or later
- **Phone Number**: A phone number for Signal registration
- **PiSovereign**: Running instance configured

## Installation

### Automatic Installation

The setup scripts can install signal-cli automatically:

**macOS:**
```bash
./scripts/setup-mac.sh
# Select "yes" when prompted for Signal support
```

**Raspberry Pi:**
```bash
sudo ./scripts/setup-pi.sh
# Select "yes" when prompted for Signal support
```

### Manual Installation

#### 1. Install Java

**macOS:**
```bash
brew install openjdk@17
sudo ln -sfn "$(brew --prefix)/opt/openjdk@17/libexec/openjdk.jdk" \
  /Library/Java/JavaVirtualMachines/openjdk-17.jdk
```

**Raspberry Pi / Debian:**
```bash
sudo apt-get install default-jdk
```

#### 2. Install signal-cli

```bash
# Download signal-cli (check for latest version)
SIGNAL_CLI_VERSION="0.13.4"
wget "https://github.com/AsamK/signal-cli/releases/download/v${SIGNAL_CLI_VERSION}/signal-cli-${SIGNAL_CLI_VERSION}.tar.gz"

# Extract
sudo mkdir -p /opt/signal-cli
sudo tar -xzf "signal-cli-${SIGNAL_CLI_VERSION}.tar.gz" -C /opt/signal-cli --strip-components=1

# Create symlink
sudo ln -sf /opt/signal-cli/bin/signal-cli /usr/local/bin/signal-cli

# Verify installation
signal-cli --version
```

## Account Registration

Before using Signal, you must register a phone number.

### Step 1: Register Phone Number

```bash
# Register your phone number (you'll receive an SMS)
signal-cli -a +1234567890 register

# Alternatively, use voice verification
signal-cli -a +1234567890 register --voice
```

### Step 2: Verify Code

```bash
# Enter the verification code from SMS
signal-cli -a +1234567890 verify 123-456
```

### Step 3: Test Registration

```bash
# Send a test message to yourself
signal-cli -a +1234567890 send -m "Hello from PiSovereign!" +1234567890
```

## Daemon Setup

signal-cli must run as a daemon to receive messages.

### macOS (launchd)

The setup script creates a launchd service automatically. To manage it:

```bash
# Start the daemon
launchctl load ~/Library/LaunchAgents/com.pisovereign.signal-cli.plist

# Check status
launchctl list | grep signal-cli

# Stop the daemon
launchctl unload ~/Library/LaunchAgents/com.pisovereign.signal-cli.plist

# View logs
tail -f ~/Library/Logs/signal-cli.log
```

### Raspberry Pi (systemd)

```bash
# Enable and start the service
sudo systemctl enable signal-cli
sudo systemctl start signal-cli

# Check status
sudo systemctl status signal-cli

# View logs
sudo journalctl -u signal-cli -f
```

## PiSovereign Configuration

### config.toml

```toml
# Select Signal as the messenger
messenger = "signal"

[signal]
# Your registered Signal phone number (E.164 format)
phone_number = "+1234567890"

# Path to signal-cli socket (default shown)
socket_path = "/var/run/signal-cli/socket"

# Path to signal-cli data directory (optional)
# data_path = "/var/lib/signal-cli"

# Connection timeout in milliseconds
timeout_ms = 30000

# Phone numbers allowed to send messages (empty = allow all)
# whitelist = ["+1234567890", "+0987654321"]
```

### Environment Variables

All settings can be overridden with environment variables:

```bash
export PISOVEREIGN_MESSENGER=signal
export PISOVEREIGN_SIGNAL__PHONE_NUMBER=+1234567890
export PISOVEREIGN_SIGNAL__SOCKET_PATH=/var/run/signal-cli/socket
export PISOVEREIGN_SIGNAL__TIMEOUT_MS=30000
```

## Message Polling

Unlike WhatsApp webhooks, Signal messages are polled from the daemon.

### API Endpoint

```bash
# Poll for new messages
curl -X POST http://localhost:3000/v1/signal/poll

# Poll with timeout (wait up to 5 seconds for new messages)
curl -X POST "http://localhost:3000/v1/signal/poll?timeout=5"
```

### Response Format

```json
{
  "processed": 2,
  "messages": [
    {
      "timestamp": 1707494400000,
      "from": "+1234567890",
      "status": "processed",
      "response": "Hello! How can I help you?",
      "response_type": "text"
    }
  ],
  "available": true
}
```

### Automatic Polling

For continuous message processing, set up a cron job or systemd timer:

**Cron (every minute):**
```bash
* * * * * curl -s -X POST http://localhost:3000/v1/signal/poll > /dev/null
```

**systemd timer (recommended):**
```bash
# /etc/systemd/system/pisovereign-signal-poll.timer
[Unit]
Description=Poll Signal messages

[Timer]
OnBootSec=30
OnUnitActiveSec=10

[Install]
WantedBy=timers.target
```

```bash
# /etc/systemd/system/pisovereign-signal-poll.service
[Unit]
Description=Poll Signal messages

[Service]
Type=oneshot
ExecStart=/usr/bin/curl -s -X POST http://localhost:3000/v1/signal/poll
```

## Health Monitoring

Check Signal daemon status:

```bash
# Health check endpoint
curl http://localhost:3000/health/signal
```

Response:
```json
{
  "available": true,
  "status": "Signal daemon is running",
  "phone_number": "***7890"
}
```

## Voice Messages

Signal voice messages are supported when the speech service is configured. Audio attachments are automatically:

1. Downloaded from the local attachment storage
2. Transcribed using STT (whisper.cpp)
3. Processed by the AI
4. Response sent as text or audio (TTS)

See [Configuration Reference](./configuration.md#speech) for speech setup.

## Troubleshooting

### Daemon Connection Failed

```
Error: Signal daemon is not available
```

**Solution:**
1. Check if the daemon is running
2. Verify socket path exists
3. Check permissions on socket directory

```bash
# Check socket
ls -la /var/run/signal-cli/socket

# Check daemon logs
# macOS:
tail -f ~/Library/Logs/signal-cli.error.log
# Linux:
journalctl -u signal-cli -f
```

### Account Not Registered

```
Error: Signal account not registered
```

**Solution:**
Follow the [Account Registration](#account-registration) steps.

### Message Send Failed

```
Error: Signal send failed: recipient not found
```

**Solution:**
- Verify the recipient has Signal installed
- Check the phone number format (E.164)

### WhitelistBlocked

If messages are being ignored:

```
[DEBUG] Ignoring message from non-whitelisted sender
```

**Solution:**
Add the sender's number to the whitelist in config.toml.

## Security Considerations

### Data Privacy

- Signal messages are end-to-end encrypted
- signal-cli stores data locally (keys, messages)
- Sensitive data directory: `/var/lib/signal-cli`

### Securing signal-cli

```bash
# Restrict data directory permissions
sudo chmod 700 /var/lib/signal-cli
sudo chown pisovereign:pisovereign /var/lib/signal-cli

# Restrict socket permissions
sudo chmod 750 /var/run/signal-cli
```

### Backup Signal Data

Include signal-cli data in backups:

```bash
# Backup location
/var/lib/signal-cli/

# Important files
/var/lib/signal-cli/data/+YOUR_NUMBER/
```

See [Backup & Restore](../operations/backup-restore.md) for full backup procedures.

## See Also

- [External Services](./external-services.md) - WhatsApp and other integrations
- [Configuration Reference](./configuration.md) - Full configuration options
- [signal-cli Documentation](https://github.com/AsamK/signal-cli) - Upstream documentation
