# Raspberry Pi Setup

> 🔧 Complete guide for setting up Raspberry Pi 5 with Hailo-10H AI HAT+ for PiSovereign

This guide covers everything from hardware assembly to a fully secured, production-ready Raspberry Pi running PiSovereign.

## Table of Contents

- [Hardware Assembly](#hardware-assembly)
  - [Required Components](#required-components)
  - [Assembly Instructions](#assembly-instructions)
- [OS Installation](#os-installation)
  - [Preparing the SD Card](#preparing-the-sd-card)
  - [First Boot Configuration](#first-boot-configuration)
- [Security Hardening](#security-hardening)
  - [SSH Configuration](#ssh-configuration)
  - [Firewall Setup (UFW)](#firewall-setup-ufw)
  - [Fail2ban Configuration](#fail2ban-configuration)
  - [Automatic Security Updates](#automatic-security-updates)
  - [Kernel Hardening](#kernel-hardening)
- [Hailo AI HAT+ Setup](#hailo-ai-hat-setup)
  - [Driver Installation](#driver-installation)
  - [Hailo-Ollama Setup](#hailo-ollama-setup)
  - [Model Installation](#model-installation)
  - [Verification](#verification)
- [PiSovereign Installation](#pisovereign-installation)
- [Systemd Service](#systemd-service)

---

## Hardware Assembly

### Required Components

| Component | Recommended Model | Notes |
|-----------|-------------------|-------|
| **Raspberry Pi 5** | 8 GB RAM variant | 4 GB works but limits concurrent operations |
| **Hailo AI HAT+ 2** | Hailo-10H (26 TOPS) | Mounts via 40-pin GPIO + PCIe |
| **Power Supply** | Official 27W USB-C | Required for HAT+ power delivery |
| **Cooling** | Active Cooler for Pi 5 | Essential for sustained AI inference |
| **Storage** | NVMe SSD (256 GB+) | Via Hailo HAT+ PCIe or separate HAT |
| **MicroSD Card** | 32 GB+ Class 10 | For boot (if not using NVMe boot) |
| **Case** | Official Pi 5 Case (tall) | Must accommodate HAT+ height |

### Assembly Instructions

> ⚠️ **Important**: Always work on a static-free surface and handle boards by edges only.

#### Step 1: Prepare the Raspberry Pi

1. Unbox the Raspberry Pi 5
2. Attach the Active Cooler:
   - Remove the protective film from the thermal pad
   - Align with the CPU and press firmly
   - Connect the 4-pin fan connector to the FAN header

#### Step 2: Install the Hailo AI HAT+

1. Locate the 40-pin GPIO header on the Pi
2. Align the Hailo HAT+ with the GPIO pins
3. Gently press down until fully seated (approximately 3mm gap)
4. Connect the PCIe FPC cable:
   - Open the Pi 5's PCIe connector latch
   - Insert the flat cable (contacts facing down)
   - Close the latch to secure

#### Step 3: Install Storage (Optional NVMe)

If using the Hailo HAT+ built-in M.2 slot:

1. Insert NVMe SSD into M.2 slot (M key, 2242/2280)
2. Secure with the provided screw

#### Step 4: Enclose and Power

1. Place assembly in case
2. Connect peripherals (keyboard, monitor for initial setup)
3. Connect power supply (do not power on yet)

---

## OS Installation

### Preparing the SD Card

#### Download Raspberry Pi Imager

On your computer (not the Pi):

```bash
# macOS
brew install --cask raspberry-pi-imager

# Ubuntu/Debian
sudo apt install rpi-imager

# Windows: Download from https://www.raspberrypi.com/software/
```

#### Flash the OS

1. Insert SD card into your computer
2. Open Raspberry Pi Imager
3. **Choose Device**: Raspberry Pi 5
4. **Choose OS**: Raspberry Pi OS Lite (64-bit) - Under "Raspberry Pi OS (other)"
5. **Choose Storage**: Select your SD card

6. Click **Edit Settings** (gear icon) and configure:

   **General tab:**
   - Set hostname: `pisovereign`
   - Set username: `pi` (or your preferred username)
   - Set password: (strong password)
   - Configure wireless LAN (optional)
   - Set locale: Your timezone

   **Services tab:**
   - Enable SSH
   - Select "Allow public-key authentication only"
   - Paste your public key (from `~/.ssh/id_ed25519.pub`)

7. Click **Save**, then **Write**

### First Boot Configuration

1. Insert SD card into Raspberry Pi
2. Connect Ethernet cable (recommended over WiFi)
3. Power on

4. Find your Pi's IP address:

```bash
# From another computer on the network
ping pisovereign.local

# Or check your router's DHCP leases
```

5. SSH into the Pi:

```bash
ssh pi@pisovereign.local
```

#### Update the System

```bash
# Update package lists and upgrade
sudo apt update && sudo apt full-upgrade -y

# Reboot to apply kernel updates
sudo reboot
```

#### Configure Boot Settings

```bash
# Open boot configuration
sudo raspi-config
```

Navigate to:
- **1 System Options** → **S6 Boot / Auto Login** → **B1 Console**
- **6 Advanced Options** → **A1 Expand Filesystem**
- **6 Advanced Options** → **A6 Boot Order** → **B2 NVMe/USB Boot** (if using NVMe)

Select **Finish** and reboot.

---

## Security Hardening

### SSH Configuration

#### Generate Ed25519 Key (on your local machine)

```bash
# Generate key pair (if you haven't already)
ssh-keygen -t ed25519 -a 100 -C "pisovereign-$(date +%Y%m%d)"

# Copy to Pi (if not done during imaging)
ssh-copy-id -i ~/.ssh/id_ed25519.pub pi@pisovereign.local
```

#### Harden SSH Daemon

```bash
# Edit SSH configuration
sudo nano /etc/ssh/sshd_config
```

Apply these settings:

```sshd_config
# Change default port (security through obscurity + reduces noise)
Port 2222

# Restrict to SSH protocol 2
Protocol 2

# Disable root login
PermitRootLogin no

# Disable password authentication (keys only)
PasswordAuthentication no
PermitEmptyPasswords no
ChallengeResponseAuthentication no

# Use strong key exchange and ciphers
KexAlgorithms curve25519-sha256@libssh.org,curve25519-sha256
Ciphers chacha20-poly1305@openssh.com,aes256-gcm@openssh.com,aes128-gcm@openssh.com
MACs hmac-sha2-512-etm@openssh.com,hmac-sha2-256-etm@openssh.com

# Limit authentication attempts
MaxAuthTries 3
MaxSessions 2
LoginGraceTime 20

# Disable unused features
X11Forwarding no
AllowTcpForwarding no
AllowAgentForwarding no
PermitUserEnvironment no

# Enable strict mode
StrictModes yes

# Log more information
LogLevel VERBOSE

# Disconnect idle sessions after 5 minutes
ClientAliveInterval 300
ClientAliveCountMax 0
```

Apply changes:

```bash
# Validate configuration
sudo sshd -t

# Restart SSH (keep current session open!)
sudo systemctl restart sshd
```

Test new connection (new terminal):

```bash
ssh -p 2222 pi@pisovereign.local
```

### Firewall Setup (UFW)

```bash
# Install UFW
sudo apt install -y ufw

# Set default policies
sudo ufw default deny incoming
sudo ufw default allow outgoing

# Allow SSH (new port)
sudo ufw allow 2222/tcp comment 'SSH'

# Allow PiSovereign HTTP API
sudo ufw allow 3000/tcp comment 'PiSovereign API'

# Allow Prometheus metrics (internal only)
sudo ufw allow from 192.168.0.0/16 to any port 8080 proto tcp comment 'Metrics'

# Enable firewall
sudo ufw enable

# Verify rules
sudo ufw status verbose
```

Expected output:

```
Status: active
Logging: on (low)
Default: deny (incoming), allow (outgoing), disabled (routed)

To                         Action      From
--                         ------      ----
2222/tcp                   ALLOW IN    Anywhere                   # SSH
3000/tcp                   ALLOW IN    Anywhere                   # PiSovereign API
8080/tcp                   ALLOW IN    192.168.0.0/16             # Metrics
```

### Fail2ban Configuration

```bash
# Install fail2ban
sudo apt install -y fail2ban

# Create local configuration
sudo nano /etc/fail2ban/jail.local
```

Add configuration:

```ini
[DEFAULT]
# Ban duration (1 hour)
bantime = 3600
# Time window for counting failures
findtime = 600
# Max failures before ban
maxretry = 3
# Action: ban IP via UFW
banaction = ufw

[sshd]
enabled = true
port = 2222
filter = sshd
logpath = /var/log/auth.log
maxretry = 3
bantime = 86400
```

Activate:

```bash
# Start and enable
sudo systemctl enable fail2ban
sudo systemctl start fail2ban

# Check status
sudo fail2ban-client status sshd
```

### Automatic Security Updates

```bash
# Install unattended-upgrades
sudo apt install -y unattended-upgrades apt-listchanges

# Enable automatic updates
sudo dpkg-reconfigure -plow unattended-upgrades
```

Configure update behavior:

```bash
sudo nano /etc/apt/apt.conf.d/50unattended-upgrades
```

Ensure these lines are uncommented:

```
Unattended-Upgrade::Origins-Pattern {
    "origin=Debian,codename=${distro_codename},label=Debian-Security";
    "origin=Raspbian,codename=${distro_codename},label=Raspbian";
};

// Automatically reboot if required
Unattended-Upgrade::Automatic-Reboot "true";
Unattended-Upgrade::Automatic-Reboot-Time "03:00";
```

### Kernel Hardening

```bash
# Add sysctl hardening
sudo nano /etc/sysctl.d/99-pisovereign-hardening.conf
```

Add:

```ini
# IP Spoofing protection
net.ipv4.conf.all.rp_filter = 1
net.ipv4.conf.default.rp_filter = 1

# Ignore ICMP broadcast requests
net.ipv4.icmp_echo_ignore_broadcasts = 1

# Disable source packet routing
net.ipv4.conf.all.accept_source_route = 0
net.ipv6.conf.all.accept_source_route = 0

# Ignore send redirects
net.ipv4.conf.all.send_redirects = 0
net.ipv4.conf.default.send_redirects = 0

# Block SYN attacks
net.ipv4.tcp_syncookies = 1
net.ipv4.tcp_max_syn_backlog = 2048
net.ipv4.tcp_synack_retries = 2

# Log Martians (impossible addresses)
net.ipv4.conf.all.log_martians = 1
net.ipv4.icmp_ignore_bogus_error_responses = 1

# Disable IPv6 if not used
net.ipv6.conf.all.disable_ipv6 = 1
net.ipv6.conf.default.disable_ipv6 = 1
```

Apply:

```bash
sudo sysctl --system
```

---

## Hailo AI HAT+ Setup

### Driver Installation

```bash
# Add Hailo APT repository
curl -fsSL https://hailo.ai/downloads/hailo-apt-key.pub | sudo gpg --dearmor -o /usr/share/keyrings/hailo.gpg

echo "deb [arch=arm64 signed-by=/usr/share/keyrings/hailo.gpg] https://apt.hailo.ai/ bookworm main" | \
  sudo tee /etc/apt/sources.list.d/hailo.list

# Update and install
sudo apt update
sudo apt install -y hailo-h10-all

# Add user to hailo group
sudo usermod -aG hailo $USER

# Reboot to load kernel modules
sudo reboot
```

### Verify Hardware Detection

After reboot:

```bash
# Check device is detected
ls -la /dev/hailo*

# Scan for Hailo devices
hailortcli scan

# Expected output:
# Hailo Devices:
# Index  Name               PCI Address
# -----  -----------------  -----------
# 0      HAILO-10H          0000:01:00.0
```

Check device info:

```bash
hailortcli fw-control identify

# Shows firmware version, serial number, etc.
```

### Hailo-Ollama Setup

Hailo-Ollama provides an OpenAI-compatible API for the Hailo NPU:

```bash
# Install hailo-ollama (if not included in hailo-h10-all)
# Check Hailo documentation for latest installation method

# Create systemd service
sudo nano /etc/systemd/system/hailo-ollama.service
```

Add service definition:

```ini
[Unit]
Description=Hailo-Ollama Inference Server
After=network.target hailort.service
Requires=hailort.service

[Service]
Type=simple
User=pi
Group=hailo
ExecStart=/usr/local/bin/hailo-ollama serve
Restart=always
RestartSec=5
Environment="HAILO_DEVICE=/dev/hailo0"
Environment="OLLAMA_HOST=127.0.0.1:11434"

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable hailo-ollama
sudo systemctl start hailo-ollama

# Check status
sudo systemctl status hailo-ollama
```

### Model Installation

```bash
# Pull recommended model
hailo-ollama pull qwen2.5-1.5b-instruct

# Alternative models:
# hailo-ollama pull llama3.2-1b-instruct
# hailo-ollama pull phi-3-mini

# List installed models
hailo-ollama list
```

### Verification

Test the inference server:

```bash
# Check API is responding
curl http://localhost:11434/api/tags

# Test inference
curl http://localhost:11434/api/generate -d '{
  "model": "qwen2.5-1.5b-instruct",
  "prompt": "Hello, how are you?",
  "stream": false
}'
```

Monitor NPU utilization:

```bash
# Watch NPU metrics
watch -n 1 hailortcli monitor
```

---

## PiSovereign Installation

### Install Dependencies

```bash
# Install build dependencies
sudo apt install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    libsqlite3-dev \
    ffmpeg

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env

# Verify Rust version
rustc --version  # Should be 1.93.0 or later
```

### Build PiSovereign

```bash
# Clone repository
git clone https://github.com/twohreichel/PiSovereign.git
cd PiSovereign

# Build release binaries
cargo build --release

# Install binaries
sudo cp target/release/pisovereign-server /usr/local/bin/
sudo cp target/release/pisovereign-cli /usr/local/bin/

# Verify installation
pisovereign-cli --version
```

### Configure

```bash
# Create configuration directory
sudo mkdir -p /etc/pisovereign

# Copy configuration template
sudo cp config.toml /etc/pisovereign/config.toml

# Create data directory
sudo mkdir -p /var/lib/pisovereign
sudo chown pi:pi /var/lib/pisovereign

# Edit configuration
sudo nano /etc/pisovereign/config.toml
```

Key configuration changes for production:

```toml
environment = "production"

[server]
host = "127.0.0.1"  # Behind reverse proxy
port = 3000
log_format = "json"

[database]
path = "/var/lib/pisovereign/pisovereign.db"

[security]
rate_limit_enabled = true
min_tls_version = "1.3"
```

See [Configuration Reference](./configuration.md) for all options.

---

## Systemd Service

Create the service file:

```bash
sudo nano /etc/systemd/system/pisovereign.service
```

Add:

```ini
[Unit]
Description=PiSovereign AI Assistant
After=network.target hailo-ollama.service
Requires=hailo-ollama.service
Wants=network-online.target

[Service]
Type=simple
User=pi
Group=pi
WorkingDirectory=/var/lib/pisovereign
ExecStart=/usr/local/bin/pisovereign-server
Restart=always
RestartSec=5

# Environment
Environment="PISOVEREIGN_CONFIG=/etc/pisovereign/config.toml"
Environment="RUST_LOG=info"

# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/var/lib/pisovereign
PrivateTmp=yes
ProtectKernelTunables=yes
ProtectControlGroups=yes
RestrictSUIDSGID=yes

# Resource limits
MemoryMax=1G
TasksMax=100

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable pisovereign
sudo systemctl start pisovereign

# Check status
sudo systemctl status pisovereign

# View logs
sudo journalctl -u pisovereign -f
```

---

## Summary

Your Raspberry Pi 5 is now:

- ✅ Securely configured with SSH key-only access on port 2222
- ✅ Protected by UFW firewall and Fail2ban
- ✅ Receiving automatic security updates
- ✅ Running Hailo AI HAT+ with hailo-ollama
- ✅ Running PiSovereign as a system service

Next steps:
- [Configure HashiCorp Vault](./vault-setup.md) for secret management
- [Set up external services](./external-services.md) (WhatsApp, email, calendar)
- [Deploy monitoring](../operations/monitoring.md)
