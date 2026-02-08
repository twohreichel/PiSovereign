#!/usr/bin/env bash
# =============================================================================
# PiSovereign - Raspberry Pi 5 Setup Script
# =============================================================================
#
# This script automates the complete setup of PiSovereign on Raspberry Pi 5:
#   - System updates and base packages
#   - Docker and Docker Compose
#   - Hailo SDK and hailo-ollama for NPU inference
#   - whisper.cpp for local speech-to-text
#   - Piper for local text-to-speech
#   - Security hardening (SSH, UFW, Fail2ban, kernel)
#   - Interactive config.toml configuration
#   - Docker Compose deployment with Traefik TLS
#   - Automatic update system via systemd timer
#
# Requirements:
#   - Raspberry Pi 5 with Hailo-10H AI HAT (recommended)
#   - Raspberry Pi OS (Debian 12 Bookworm) or Ubuntu 24.04
#   - Root privileges (run with sudo)
#   - Internet connection
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/twohreichel/PiSovereign/main/scripts/setup-pi.sh | sudo bash
#
# =============================================================================

set -euo pipefail

# =============================================================================
# Configuration
# =============================================================================

PISOVEREIGN_VERSION="${PISOVEREIGN_VERSION:-latest}"
PISOVEREIGN_DIR="/opt/pisovereign"
PISOVEREIGN_CONFIG_DIR="/etc/pisovereign"
PISOVEREIGN_DATA_DIR="/var/lib/pisovereign"
PISOVEREIGN_LOG_DIR="/var/log/pisovereign"

WHISPER_MODEL_DIR="/usr/local/share/whisper"
WHISPER_MODEL="ggml-base.bin"
WHISPER_MODEL_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin"

PIPER_DIR="/usr/local/share/piper"
PIPER_VERSION="2023.11.14-2"
PIPER_VOICE="de_DE-thorsten-medium"
PIPER_VOICE_URL="https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/de/de_DE/thorsten/medium"

OLLAMA_MODEL="qwen2.5:1.5b"

# =============================================================================
# Colors and Output
# =============================================================================

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

info() { echo -e "${BLUE}[INFO]${NC} $*"; }
success() { echo -e "${GREEN}[OK]${NC} $*"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }
step() { echo -e "\n${PURPLE}==>${NC} ${CYAN}$*${NC}"; }

# =============================================================================
# Utility Functions
# =============================================================================

check_root() {
    if [[ $EUID -ne 0 ]]; then
        error "This script must be run as root (use sudo)"
        exit 1
    fi
}

check_platform() {
    if [[ ! -f /proc/device-tree/model ]]; then
        warn "Could not detect Raspberry Pi model"
        return
    fi
    
    local model
    model=$(tr -d '\0' < /proc/device-tree/model)
    
    if [[ "$model" == *"Raspberry Pi 5"* ]]; then
        success "Detected: $model"
    else
        warn "Detected: $model (Raspberry Pi 5 recommended for optimal performance)"
    fi
}

check_hailo() {
    if [[ -e /dev/hailo0 ]]; then
        success "Hailo NPU detected at /dev/hailo0"
        return 0
    else
        warn "Hailo NPU not detected - will use CPU inference (slower)"
        return 1
    fi
}

prompt_yes_no() {
    local prompt="$1"
    local default="${2:-y}"
    local answer
    
    if [[ "$default" == "y" ]]; then
        read -rp "$prompt [Y/n]: " answer
        answer="${answer:-y}"
    else
        read -rp "$prompt [y/N]: " answer
        answer="${answer:-n}"
    fi
    
    [[ "${answer,,}" == "y" || "${answer,,}" == "yes" ]]
}

prompt_input() {
    local prompt="$1"
    local default="${2:-}"
    local answer
    
    if [[ -n "$default" ]]; then
        read -rp "$prompt [$default]: " answer
        echo "${answer:-$default}"
    else
        read -rp "$prompt: " answer
        echo "$answer"
    fi
}

prompt_secret() {
    local prompt="$1"
    local answer
    read -rsp "$prompt: " answer
    echo
    echo "$answer"
}

# =============================================================================
# Installation Functions
# =============================================================================

install_base_packages() {
    step "Installing base packages"
    
    apt-get update
    apt-get upgrade -y
    
    apt-get install -y \
        build-essential \
        pkg-config \
        libssl-dev \
        libsqlite3-dev \
        ffmpeg \
        curl \
        wget \
        jq \
        git \
        cmake \
        ca-certificates \
        gnupg \
        lsb-release \
        ufw \
        fail2ban \
        unattended-upgrades \
        apt-listchanges \
        logrotate
    
    success "Base packages installed"
}

install_docker() {
    step "Installing Docker"
    
    if command -v docker &>/dev/null; then
        success "Docker already installed: $(docker --version)"
        return
    fi
    
    # Install Docker using official script
    curl -fsSL https://get.docker.com | sh
    
    # Add current user to docker group
    local real_user="${SUDO_USER:-$USER}"
    usermod -aG docker "$real_user"
    
    # Enable and start Docker
    systemctl enable docker
    systemctl start docker
    
    # Install Docker Compose plugin if not present
    if ! docker compose version &>/dev/null; then
        apt-get install -y docker-compose-plugin
    fi
    
    success "Docker installed: $(docker --version)"
}

install_hailo_sdk() {
    step "Setting up Hailo SDK"
    
    # Check if Hailo is already installed
    if command -v hailortcli &>/dev/null; then
        success "Hailo SDK already installed"
        return
    fi
    
    # Check if Hailo hardware is present
    if ! check_hailo; then
        warn "Skipping Hailo SDK installation (no hardware detected)"
        return
    fi
    
    info "Installing Hailo SDK packages..."
    
    # Add Hailo repository if not present
    if [[ ! -f /etc/apt/sources.list.d/hailo.list ]]; then
        curl -fsSL https://hailo.ai/debian/hailo.gpg | gpg --dearmor -o /usr/share/keyrings/hailo-archive-keyring.gpg
        echo "deb [arch=arm64 signed-by=/usr/share/keyrings/hailo-archive-keyring.gpg] https://hailo.ai/debian bookworm main" > /etc/apt/sources.list.d/hailo.list
        apt-get update
    fi
    
    # Install Hailo packages
    apt-get install -y hailo-h10-all || {
        warn "Could not install hailo-h10-all, trying individual packages..."
        apt-get install -y hailort hailo-firmware || true
    }
    
    # Create hailo group and add pisovereign user
    groupadd -f hailo
    
    # Set device permissions
    if [[ -e /dev/hailo0 ]]; then
        chgrp hailo /dev/hailo0
        chmod 660 /dev/hailo0
    fi
    
    success "Hailo SDK installed"
}

install_ollama() {
    step "Installing Ollama"
    
    if command -v ollama &>/dev/null; then
        success "Ollama already installed"
    else
        # Install Ollama
        curl -fsSL https://ollama.com/install.sh | sh
        success "Ollama installed"
    fi
    
    # Enable and start Ollama service
    systemctl enable ollama
    systemctl start ollama
    
    # Wait for Ollama to be ready
    info "Waiting for Ollama to start..."
    local retries=30
    while ! curl -s http://localhost:11434/api/tags &>/dev/null; do
        sleep 1
        ((retries--)) || {
            error "Ollama failed to start"
            return 1
        }
    done
    
    # Pull default model
    info "Pulling LLM model: $OLLAMA_MODEL (this may take a while)..."
    ollama pull "$OLLAMA_MODEL"
    
    success "Ollama ready with model: $OLLAMA_MODEL"
}

install_whisper_cpp() {
    step "Installing whisper.cpp"
    
    if command -v whisper-cpp &>/dev/null || command -v whisper &>/dev/null; then
        success "whisper.cpp already installed"
    else
        info "Building whisper.cpp from source..."
        
        local build_dir="/tmp/whisper-build"
        rm -rf "$build_dir"
        git clone https://github.com/ggerganov/whisper.cpp.git "$build_dir"
        cd "$build_dir"
        
        # Build with optimizations for ARM64
        cmake -B build \
            -DCMAKE_BUILD_TYPE=Release \
            -DWHISPER_BUILD_EXAMPLES=ON \
            -DWHISPER_BUILD_TESTS=OFF
        cmake --build build --config Release -j$(nproc)
        
        # Install binary
        cp build/bin/whisper-cli /usr/local/bin/whisper-cpp
        chmod +x /usr/local/bin/whisper-cpp
        
        cd /
        rm -rf "$build_dir"
        
        success "whisper.cpp built and installed"
    fi
    
    # Download model
    mkdir -p "$WHISPER_MODEL_DIR"
    
    if [[ ! -f "$WHISPER_MODEL_DIR/$WHISPER_MODEL" ]]; then
        info "Downloading Whisper model: $WHISPER_MODEL..."
        wget -q --show-progress -O "$WHISPER_MODEL_DIR/$WHISPER_MODEL" "$WHISPER_MODEL_URL"
        success "Whisper model downloaded"
    else
        success "Whisper model already present"
    fi
}

install_piper() {
    step "Installing Piper TTS"
    
    if command -v piper &>/dev/null; then
        success "Piper already installed"
    else
        info "Downloading Piper..."
        
        local piper_url="https://github.com/rhasspy/piper/releases/download/${PIPER_VERSION}/piper_linux_aarch64.tar.gz"
        local temp_dir="/tmp/piper-install"
        
        rm -rf "$temp_dir"
        mkdir -p "$temp_dir"
        
        wget -q --show-progress -O "$temp_dir/piper.tar.gz" "$piper_url"
        tar -xzf "$temp_dir/piper.tar.gz" -C "$temp_dir"
        
        # Install binary and libraries
        cp "$temp_dir/piper/piper" /usr/local/bin/
        chmod +x /usr/local/bin/piper
        
        # Copy espeak-ng data if present
        if [[ -d "$temp_dir/piper/espeak-ng-data" ]]; then
            cp -r "$temp_dir/piper/espeak-ng-data" /usr/local/share/
        fi
        
        rm -rf "$temp_dir"
        success "Piper installed"
    fi
    
    # Download voice model
    mkdir -p "$PIPER_DIR"
    
    local voice_onnx="$PIPER_DIR/${PIPER_VOICE}.onnx"
    local voice_json="$PIPER_DIR/${PIPER_VOICE}.onnx.json"
    
    if [[ ! -f "$voice_onnx" ]]; then
        info "Downloading Piper voice: $PIPER_VOICE..."
        wget -q --show-progress -O "$voice_onnx" "${PIPER_VOICE_URL}/${PIPER_VOICE}.onnx"
        wget -q --show-progress -O "$voice_json" "${PIPER_VOICE_URL}/${PIPER_VOICE}.onnx.json"
        success "Piper voice downloaded"
    else
        success "Piper voice already present"
    fi
}

# =============================================================================
# Security Hardening
# =============================================================================

setup_pisovereign_user() {
    step "Creating PiSovereign system user"
    
    if id pisovereign &>/dev/null; then
        success "User 'pisovereign' already exists"
    else
        useradd -r -s /usr/sbin/nologin -d "$PISOVEREIGN_DATA_DIR" pisovereign
        success "Created system user 'pisovereign'"
    fi
    
    # Add to required groups
    usermod -aG docker pisovereign 2>/dev/null || true
    usermod -aG hailo pisovereign 2>/dev/null || true
    
    # Create directories
    mkdir -p "$PISOVEREIGN_DIR" "$PISOVEREIGN_CONFIG_DIR" "$PISOVEREIGN_DATA_DIR" "$PISOVEREIGN_LOG_DIR"
    chown pisovereign:pisovereign "$PISOVEREIGN_DATA_DIR" "$PISOVEREIGN_LOG_DIR"
    chmod 750 "$PISOVEREIGN_DATA_DIR"
    chmod 755 "$PISOVEREIGN_CONFIG_DIR" "$PISOVEREIGN_LOG_DIR"
}

setup_ssh_hardening() {
    step "Hardening SSH configuration"
    
    if ! prompt_yes_no "Apply SSH hardening (port 2222, key-only auth)?"; then
        warn "Skipping SSH hardening"
        return
    fi
    
    local ssh_config="/etc/ssh/sshd_config"
    local ssh_backup="/etc/ssh/sshd_config.backup.$(date +%Y%m%d%H%M%S)"
    
    # Backup original config
    cp "$ssh_config" "$ssh_backup"
    info "Backed up SSH config to $ssh_backup"
    
    # Create hardened config
    cat > "$ssh_config" << 'EOF'
# PiSovereign Hardened SSH Configuration
Port 2222
Protocol 2

# Authentication
PermitRootLogin no
PubkeyAuthentication yes
PasswordAuthentication no
PermitEmptyPasswords no
ChallengeResponseAuthentication no
UsePAM yes

# Key exchange and ciphers (secure only)
KexAlgorithms curve25519-sha256@libssh.org,diffie-hellman-group16-sha512,diffie-hellman-group18-sha512
Ciphers chacha20-poly1305@openssh.com,aes256-gcm@openssh.com,aes128-gcm@openssh.com
MACs hmac-sha2-512-etm@openssh.com,hmac-sha2-256-etm@openssh.com

# Connection settings
LoginGraceTime 30
MaxAuthTries 3
MaxSessions 3
MaxStartups 3:50:10

# Idle timeout
ClientAliveInterval 300
ClientAliveCountMax 2

# Disable forwarding
AllowTcpForwarding no
X11Forwarding no
AllowAgentForwarding no

# Logging
LogLevel VERBOSE
EOF

    # Restart SSH
    systemctl restart sshd
    
    success "SSH hardened (port 2222, key-only authentication)"
    warn "IMPORTANT: Ensure you have SSH keys configured before closing this session!"
}

setup_firewall() {
    step "Configuring UFW firewall"
    
    # Reset UFW
    ufw --force reset
    
    # Default policies
    ufw default deny incoming
    ufw default allow outgoing
    
    # Allow SSH (check if we changed the port)
    if grep -q "^Port 2222" /etc/ssh/sshd_config 2>/dev/null; then
        ufw allow 2222/tcp comment 'SSH'
    else
        ufw allow 22/tcp comment 'SSH'
    fi
    
    # Allow HTTP/HTTPS for Traefik
    ufw allow 80/tcp comment 'HTTP (Let'\''s Encrypt)'
    ufw allow 443/tcp comment 'HTTPS'
    
    # Enable firewall
    ufw --force enable
    
    success "UFW firewall configured"
    ufw status verbose
}

setup_fail2ban() {
    step "Configuring Fail2ban"
    
    # Create jail.local
    cat > /etc/fail2ban/jail.local << 'EOF'
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

[pisovereign]
enabled = true
port = 443
filter = pisovereign
logpath = /var/log/pisovereign/access.log
maxretry = 10
findtime = 1m
bantime = 1h
EOF

    # Create PiSovereign filter
    cat > /etc/fail2ban/filter.d/pisovereign.conf << 'EOF'
[Definition]
failregex = ^.* "request_id":"[^"]*","remote_addr":"<HOST>".* "status":(401|403|429).*$
ignoreregex =
EOF

    # Restart fail2ban
    systemctl enable fail2ban
    systemctl restart fail2ban
    
    success "Fail2ban configured"
}

setup_kernel_hardening() {
    step "Applying kernel hardening"
    
    cat > /etc/sysctl.d/99-pisovereign-hardening.conf << 'EOF'
# PiSovereign Kernel Hardening

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

# Memory protection
kernel.randomize_va_space = 2
kernel.kptr_restrict = 2
kernel.yama.ptrace_scope = 1

# Filesystem hardening
fs.protected_hardlinks = 1
fs.protected_symlinks = 1
fs.suid_dumpable = 0
EOF

    sysctl --system
    
    success "Kernel hardening applied"
}

setup_auto_updates() {
    step "Configuring automatic security updates"
    
    # Configure unattended-upgrades
    cat > /etc/apt/apt.conf.d/50unattended-upgrades << 'EOF'
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
EOF

    # Enable automatic updates
    cat > /etc/apt/apt.conf.d/20auto-upgrades << 'EOF'
APT::Periodic::Update-Package-Lists "1";
APT::Periodic::Unattended-Upgrade "1";
APT::Periodic::AutocleanInterval "7";
EOF

    success "Automatic security updates enabled"
}

# =============================================================================
# Configuration
# =============================================================================

configure_toml() {
    step "Configuring PiSovereign"
    
    echo
    info "Please provide the following configuration values."
    info "Press Enter to use defaults shown in [brackets]."
    echo
    
    # Server configuration
    local server_host
    local server_port
    local domain
    server_host=$(prompt_input "Server bind address" "0.0.0.0")
    server_port=$(prompt_input "Server port" "3000")
    domain=$(prompt_input "Domain name (for TLS, leave empty for local only)" "")
    
    # LLM configuration
    local llm_model
    llm_model=$(prompt_input "LLM model" "$OLLAMA_MODEL")
    
    # Speech configuration
    local speech_provider
    echo
    info "Speech provider options: local, openai, hybrid"
    speech_provider=$(prompt_input "Speech provider" "local")
    
    local openai_api_key=""
    if [[ "$speech_provider" == "openai" || "$speech_provider" == "hybrid" ]]; then
        openai_api_key=$(prompt_secret "OpenAI API key")
    fi
    
    # WhatsApp configuration (optional)
    local whatsapp_enabled=false
    local whatsapp_access_token=""
    local whatsapp_phone_id=""
    local whatsapp_app_secret=""
    local whatsapp_verify_token=""
    
    echo
    if prompt_yes_no "Configure WhatsApp integration?" "n"; then
        whatsapp_enabled=true
        whatsapp_access_token=$(prompt_secret "WhatsApp access token")
        whatsapp_phone_id=$(prompt_input "WhatsApp phone number ID")
        whatsapp_app_secret=$(prompt_secret "WhatsApp app secret")
        whatsapp_verify_token=$(prompt_input "WhatsApp verify token" "pisovereign-verify")
    fi
    
    # Weather configuration
    local weather_lat
    local weather_lon
    echo
    info "Default location for weather (Berlin: 52.52, 13.405)"
    weather_lat=$(prompt_input "Latitude" "52.52")
    weather_lon=$(prompt_input "Longitude" "13.405")
    
    # Generate config.toml
    info "Generating configuration..."
    
    cat > "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF
# PiSovereign Configuration
# Generated by setup-pi.sh on $(date -Iseconds)

environment = "production"

[server]
host = "$server_host"
port = $server_port
cors_enabled = true
allowed_origins = []
shutdown_timeout_secs = 30
log_format = "json"

[inference]
base_url = "http://localhost:11434"
default_model = "$llm_model"
timeout_ms = 60000
max_tokens = 2048
temperature = 0.7
top_p = 0.9

[security]
whitelisted_phones = []
rate_limit_enabled = true
rate_limit_rpm = 60
tls_verify_certs = true
connection_timeout_secs = 30
min_tls_version = "1.2"

[database]
path = "$PISOVEREIGN_DATA_DIR/pisovereign.db"
max_connections = 5
run_migrations = true

[cache]
enabled = true
ttl_short_secs = 300
ttl_medium_secs = 3600
ttl_long_secs = 86400
l1_max_entries = 10000

[telemetry]
enabled = false

[degraded_mode]
enabled = true
unavailable_message = "I'm currently experiencing technical difficulties. Please try again in a moment."
retry_cooldown_secs = 30
failure_threshold = 3
success_threshold = 2

[retry]
initial_delay_ms = 100
max_delay_ms = 10000
multiplier = 2.0
max_retries = 3

[health]
global_timeout_secs = 5
EOF

    # Add speech configuration
    if [[ "$speech_provider" != "" ]]; then
        cat >> "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF

[speech]
provider = "$speech_provider"
EOF
        
        if [[ -n "$openai_api_key" ]]; then
            cat >> "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF
openai_api_key = "$openai_api_key"
stt_model = "whisper-1"
tts_model = "tts-1"
default_voice = "nova"
EOF
        fi
        
        if [[ "$speech_provider" == "local" || "$speech_provider" == "hybrid" ]]; then
            cat >> "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF

[speech.local_stt]
executable_path = "whisper-cpp"
model_path = "$WHISPER_MODEL_DIR/$WHISPER_MODEL"
threads = 4
default_language = "de"

[speech.local_tts]
executable_path = "piper"
default_model_path = "$PIPER_DIR/${PIPER_VOICE}.onnx"
default_voice = "$PIPER_VOICE"
EOF
        fi
    fi
    
    # Add WhatsApp configuration
    if [[ "$whatsapp_enabled" == true ]]; then
        cat >> "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF

[whatsapp]
access_token = "$whatsapp_access_token"
phone_number_id = "$whatsapp_phone_id"
app_secret = "$whatsapp_app_secret"
verify_token = "$whatsapp_verify_token"
signature_required = true
api_version = "v18.0"
EOF
    fi
    
    # Add weather configuration
    cat >> "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF

[weather]
base_url = "https://api.open-meteo.com/v1"
timeout_secs = 30
forecast_days = 7
cache_ttl_minutes = 30
default_location = { latitude = $weather_lat, longitude = $weather_lon }
EOF

    # Set permissions
    chmod 640 "$PISOVEREIGN_CONFIG_DIR/config.toml"
    chown root:pisovereign "$PISOVEREIGN_CONFIG_DIR/config.toml"
    
    success "Configuration saved to $PISOVEREIGN_CONFIG_DIR/config.toml"
    
    # Store domain for docker-compose
    if [[ -n "$domain" ]]; then
        echo "$domain" > "$PISOVEREIGN_DIR/.domain"
    fi
}

# =============================================================================
# Docker Compose Setup
# =============================================================================

setup_docker_compose() {
    step "Setting up Docker Compose"
    
    local domain=""
    if [[ -f "$PISOVEREIGN_DIR/.domain" ]]; then
        domain=$(cat "$PISOVEREIGN_DIR/.domain")
    fi
    
    mkdir -p "$PISOVEREIGN_DIR"
    cd "$PISOVEREIGN_DIR"
    
    # Create docker-compose.yml
    cat > docker-compose.yml << 'EOF'
version: '3.8'

services:
  pisovereign:
    image: ghcr.io/twohreichel/pisovereign:latest
    container_name: pisovereign
    restart: unless-stopped
    ports:
      - "127.0.0.1:3000:3000"
      - "127.0.0.1:8080:8080"
    volumes:
      - /etc/pisovereign/config.toml:/etc/pisovereign/config.toml:ro
      - /var/lib/pisovereign:/var/lib/pisovereign
      - /var/log/pisovereign:/var/log/pisovereign
    environment:
      - PISOVEREIGN_CONFIG=/etc/pisovereign/config.toml
      - PISOVEREIGN_ENVIRONMENT=production
      - RUST_LOG=info
    extra_hosts:
      - "host.docker.internal:host-gateway"
    depends_on:
      - ollama
    networks:
      - pisovereign-net
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 10s

  ollama:
    image: ollama/ollama:latest
    container_name: ollama
    restart: unless-stopped
    ports:
      - "127.0.0.1:11434:11434"
    volumes:
      - ollama-models:/root/.ollama
    networks:
      - pisovereign-net

networks:
  pisovereign-net:
    driver: bridge

volumes:
  ollama-models:
EOF

    # Add Traefik if domain is configured
    if [[ -n "$domain" ]]; then
        info "Adding Traefik configuration for domain: $domain"
        
        local acme_email
        acme_email=$(prompt_input "Email for Let's Encrypt certificates")
        
        cat > docker-compose.override.yml << EOF
version: '3.8'

services:
  pisovereign:
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.pisovereign.rule=Host(\`$domain\`)"
      - "traefik.http.routers.pisovereign.entrypoints=websecure"
      - "traefik.http.routers.pisovereign.tls.certresolver=letsencrypt"
      - "traefik.http.services.pisovereign.loadbalancer.server.port=3000"
      - "traefik.http.middlewares.pisovereign-security.headers.stsSeconds=31536000"
      - "traefik.http.middlewares.pisovereign-security.headers.stsIncludeSubdomains=true"
      - "traefik.http.middlewares.pisovereign-security.headers.contentTypeNosniff=true"
      - "traefik.http.middlewares.pisovereign-security.headers.frameDeny=true"
      - "traefik.http.routers.pisovereign.middlewares=pisovereign-security"
    networks:
      - pisovereign-net

  traefik:
    image: traefik:v3.0
    container_name: traefik
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
    command:
      - "--api.dashboard=false"
      - "--entrypoints.web.address=:80"
      - "--entrypoints.websecure.address=:443"
      - "--entrypoints.web.http.redirections.entrypoint.to=websecure"
      - "--entrypoints.web.http.redirections.entrypoint.scheme=https"
      - "--certificatesresolvers.letsencrypt.acme.tlschallenge=true"
      - "--certificatesresolvers.letsencrypt.acme.email=$acme_email"
      - "--certificatesresolvers.letsencrypt.acme.storage=/letsencrypt/acme.json"
      - "--providers.docker=true"
      - "--providers.docker.exposedbydefault=false"
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - traefik-letsencrypt:/letsencrypt
    networks:
      - pisovereign-net

volumes:
  traefik-letsencrypt:
EOF
    fi
    
    success "Docker Compose configuration created"
}

start_services() {
    step "Starting PiSovereign services"
    
    cd "$PISOVEREIGN_DIR"
    
    # Pull images
    info "Pulling Docker images..."
    docker compose pull
    
    # Start services
    info "Starting services..."
    docker compose up -d
    
    # Wait for health check
    info "Waiting for services to be healthy..."
    local retries=60
    while ! curl -sf http://localhost:3000/health &>/dev/null; do
        sleep 2
        ((retries--)) || {
            error "PiSovereign failed to start"
            docker compose logs pisovereign
            return 1
        }
    done
    
    success "PiSovereign is running!"
}

# =============================================================================
# Auto-Update System
# =============================================================================

setup_auto_update_service() {
    step "Setting up automatic updates"
    
    # Create update script
    cat > "$PISOVEREIGN_DIR/scripts/auto-update.sh" << 'EOF'
#!/usr/bin/env bash
# PiSovereign Auto-Update Script

set -euo pipefail

LOG_FILE="/var/log/pisovereign/auto-update.log"
PISOVEREIGN_DIR="/opt/pisovereign"

log() {
    echo "[$(date -Iseconds)] $*" | tee -a "$LOG_FILE"
}

log "Starting auto-update..."

# Update system packages
log "Updating system packages..."
apt-get update -qq
apt-get upgrade -y -qq

# Update Docker images
log "Updating Docker images..."
cd "$PISOVEREIGN_DIR"
docker compose pull -q

# Restart services if images changed
if docker compose up -d --remove-orphans 2>&1 | grep -q "Recreating\|Creating"; then
    log "Services restarted with new images"
else
    log "No image updates"
fi

# Update LLM model
log "Checking LLM model updates..."
ollama pull "$OLLAMA_MODEL" 2>&1 | grep -v "up to date" || true

# Cleanup
log "Cleaning up..."
docker system prune -f --volumes 2>/dev/null || true
apt-get autoremove -y -qq
apt-get autoclean -qq

log "Auto-update completed"
EOF

    chmod +x "$PISOVEREIGN_DIR/scripts/auto-update.sh"
    mkdir -p "$PISOVEREIGN_DIR/scripts"
    mv "$PISOVEREIGN_DIR/scripts/auto-update.sh" "$PISOVEREIGN_DIR/scripts/auto-update.sh" 2>/dev/null || true
    
    # Create systemd service
    cat > /etc/systemd/system/pisovereign-update.service << EOF
[Unit]
Description=PiSovereign Auto-Update
After=network-online.target docker.service
Wants=network-online.target

[Service]
Type=oneshot
ExecStart=$PISOVEREIGN_DIR/scripts/auto-update.sh
User=root
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF

    # Create systemd timer
    cat > /etc/systemd/system/pisovereign-update.timer << 'EOF'
[Unit]
Description=Run PiSovereign Auto-Update daily

[Timer]
OnCalendar=*-*-* 03:00:00
RandomizedDelaySec=1800
Persistent=true

[Install]
WantedBy=timers.target
EOF

    # Enable timer
    systemctl daemon-reload
    systemctl enable pisovereign-update.timer
    systemctl start pisovereign-update.timer
    
    success "Auto-update configured (daily at 03:00)"
}

# Create logrotate configuration
setup_logrotate() {
    cat > /etc/logrotate.d/pisovereign << 'EOF'
/var/log/pisovereign/*.log {
    daily
    missingok
    rotate 14
    compress
    delaycompress
    notifempty
    create 0640 pisovereign pisovereign
    sharedscripts
    postrotate
        docker kill --signal=USR1 pisovereign 2>/dev/null || true
    endscript
}
EOF
    
    success "Log rotation configured"
}

# =============================================================================
# Verification
# =============================================================================

verify_installation() {
    step "Verifying installation"
    
    local errors=0
    
    # Check Docker
    if docker info &>/dev/null; then
        success "Docker: OK"
    else
        error "Docker: FAILED"
        ((errors++))
    fi
    
    # Check Ollama
    if curl -sf http://localhost:11434/api/tags &>/dev/null; then
        success "Ollama: OK"
    else
        error "Ollama: FAILED"
        ((errors++))
    fi
    
    # Check whisper.cpp
    if command -v whisper-cpp &>/dev/null || command -v whisper &>/dev/null; then
        success "whisper.cpp: OK"
    else
        error "whisper.cpp: FAILED"
        ((errors++))
    fi
    
    # Check Piper
    if command -v piper &>/dev/null; then
        success "Piper: OK"
    else
        error "Piper: FAILED"
        ((errors++))
    fi
    
    # Check PiSovereign
    if curl -sf http://localhost:3000/health &>/dev/null; then
        success "PiSovereign: OK"
    else
        error "PiSovereign: FAILED"
        ((errors++))
    fi
    
    # Check auto-update timer
    if systemctl is-active pisovereign-update.timer &>/dev/null; then
        success "Auto-update timer: OK"
    else
        warn "Auto-update timer: NOT ACTIVE"
    fi
    
    return $errors
}

print_summary() {
    local domain=""
    if [[ -f "$PISOVEREIGN_DIR/.domain" ]]; then
        domain=$(cat "$PISOVEREIGN_DIR/.domain")
    fi
    
    echo
    echo -e "${GREEN}============================================${NC}"
    echo -e "${GREEN}    PiSovereign Installation Complete!     ${NC}"
    echo -e "${GREEN}============================================${NC}"
    echo
    echo -e "${CYAN}Configuration:${NC}"
    echo "  Config file:    $PISOVEREIGN_CONFIG_DIR/config.toml"
    echo "  Data directory: $PISOVEREIGN_DATA_DIR"
    echo "  Log directory:  $PISOVEREIGN_LOG_DIR"
    echo
    echo -e "${CYAN}Services:${NC}"
    echo "  PiSovereign:    http://localhost:3000"
    echo "  Metrics:        http://localhost:8080/metrics"
    echo "  Ollama:         http://localhost:11434"
    
    if [[ -n "$domain" ]]; then
        echo "  Public URL:     https://$domain"
    fi
    
    echo
    echo -e "${CYAN}Management:${NC}"
    echo "  View logs:      docker compose -f $PISOVEREIGN_DIR/docker-compose.yml logs -f"
    echo "  Restart:        docker compose -f $PISOVEREIGN_DIR/docker-compose.yml restart"
    echo "  Update now:     $PISOVEREIGN_DIR/scripts/auto-update.sh"
    echo "  Update timer:   systemctl status pisovereign-update.timer"
    echo
    echo -e "${CYAN}Security:${NC}"
    echo "  SSH port:       2222 (if hardening was applied)"
    echo "  Firewall:       ufw status"
    echo "  Fail2ban:       fail2ban-client status"
    echo
    echo -e "${YELLOW}Next steps:${NC}"
    echo "  1. Configure SSH keys if you enabled SSH hardening"
    echo "  2. Test the API: curl http://localhost:3000/health"
    echo "  3. Set up WhatsApp webhook if using WhatsApp integration"
    echo "  4. Review logs: docker compose logs -f"
    echo
}

# =============================================================================
# Main
# =============================================================================

main() {
    echo
    echo -e "${PURPLE}╔═══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${PURPLE}║         PiSovereign Setup Script for Raspberry Pi         ║${NC}"
    echo -e "${PURPLE}╚═══════════════════════════════════════════════════════════╝${NC}"
    echo
    
    check_root
    check_platform
    
    echo
    info "This script will install and configure PiSovereign on your Raspberry Pi."
    info "It will install Docker, Ollama, whisper.cpp, Piper, and apply security hardening."
    echo
    
    if ! prompt_yes_no "Continue with installation?"; then
        info "Installation cancelled"
        exit 0
    fi
    
    # Installation
    install_base_packages
    install_docker
    install_hailo_sdk
    install_ollama
    install_whisper_cpp
    install_piper
    
    # Security
    setup_pisovereign_user
    setup_ssh_hardening
    setup_firewall
    setup_fail2ban
    setup_kernel_hardening
    setup_auto_updates
    
    # Configuration
    configure_toml
    
    # Deployment
    setup_docker_compose
    start_services
    
    # Auto-update
    mkdir -p "$PISOVEREIGN_DIR/scripts"
    setup_auto_update_service
    setup_logrotate
    
    # Verification
    if verify_installation; then
        print_summary
    else
        echo
        error "Installation completed with errors. Please check the logs."
        exit 1
    fi
}

main "$@"
