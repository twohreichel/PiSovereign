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

# Deployment mode: "native" (default for Pi) or "docker"
DEPLOY_MODE="${DEPLOY_MODE:-native}"

# Git repository for source builds
PISOVEREIGN_REPO="https://github.com/twohreichel/PiSovereign.git"
PISOVEREIGN_BRANCH="${PISOVEREIGN_BRANCH:-main}"

# Rust toolchain
RUST_VERSION="1.83.0"

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
# CLI Argument Parsing
# =============================================================================

show_help() {
    cat << EOF
PiSovereign Setup Script for Raspberry Pi

Usage: $0 [OPTIONS]

Options:
    --native        Build from source and install native binaries (default)
    --docker        Use Docker containers instead of native binaries
    --monitoring    Install Prometheus + Grafana monitoring stack
    --otel          Install OpenTelemetry Collector (Docker)
    --baikal        Install Baïkal CalDAV server (Docker)
    --branch NAME   Git branch to build from (default: main)
    --skip-security Skip security hardening steps
    --skip-build    Skip building (use pre-built binaries from GitHub releases)
    -h, --help      Show this help message

Examples:
    sudo $0                      # Native build (recommended for Pi)
    sudo $0 --monitoring         # With Prometheus + Grafana monitoring
    sudo $0 --otel               # With OpenTelemetry Collector
    sudo $0 --monitoring --otel  # Full observability stack
    sudo $0 --baikal             # With Baïkal CalDAV server
    sudo $0 --docker             # Docker deployment
    sudo $0 --branch develop     # Build from develop branch
    sudo $0 --skip-security      # Skip SSH/firewall hardening

Environment Variables:
    DEPLOY_MODE         native or docker (default: native)
    PISOVEREIGN_VERSION Version tag to install (default: latest)
    PISOVEREIGN_BRANCH  Git branch for source builds (default: main)
    INSTALL_MONITORING  true or false (default: false)
    INSTALL_OTEL        true or false (default: false)
    INSTALL_BAIKAL      true or false (default: false)

EOF
    exit 0
}

parse_args() {
    SKIP_SECURITY=false
    SKIP_BUILD=false
    INSTALL_MONITORING=${INSTALL_MONITORING:-false}
    INSTALL_OTEL=${INSTALL_OTEL:-false}
    INSTALL_BAIKAL=${INSTALL_BAIKAL:-false}
    
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --native)
                DEPLOY_MODE="native"
                shift
                ;;
            --docker)
                DEPLOY_MODE="docker"
                shift
                ;;
            --monitoring)
                INSTALL_MONITORING=true
                shift
                ;;
            --otel)
                INSTALL_OTEL=true
                shift
                ;;
            --baikal)
                INSTALL_BAIKAL=true
                shift
                ;;
            --branch)
                PISOVEREIGN_BRANCH="$2"
                shift 2
                ;;
            --skip-security)
                SKIP_SECURITY=true
                shift
                ;;
            --skip-build)
                SKIP_BUILD=true
                shift
                ;;
            -h|--help)
                show_help
                ;;
            *)
                error "Unknown option: $1"
                show_help
                ;;
        esac
    done
}

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

install_signal_cli() {
    step "Installing signal-cli (for Signal messenger integration)"
    
    # Check if user wants Signal integration
    if ! prompt_yes_no "Install signal-cli for Signal messenger support?" "n"; then
        info "Skipping signal-cli installation"
        return
    fi
    
    # Install OpenJDK 17 (required for signal-cli)
    if ! command -v java &>/dev/null || ! java -version 2>&1 | grep -q "17\|21"; then
        info "Installing OpenJDK 17 (required for signal-cli)..."
        apt-get install -y default-jdk
        success "OpenJDK installed"
    else
        success "Java runtime already installed"
    fi
    
    # Determine signal-cli version and install location
    local signal_cli_version="0.13.4"
    local signal_cli_dir="/opt/signal-cli"
    local signal_cli_bin="/usr/local/bin/signal-cli"
    
    if [[ -x "$signal_cli_bin" ]]; then
        success "signal-cli already installed"
    else
        info "Downloading signal-cli v${signal_cli_version}..."
        
        local signal_cli_url="https://github.com/AsamK/signal-cli/releases/download/v${signal_cli_version}/signal-cli-${signal_cli_version}.tar.gz"
        local temp_dir="/tmp/signal-cli-install"
        
        rm -rf "$temp_dir"
        mkdir -p "$temp_dir" "$signal_cli_dir"
        
        wget -q --show-progress -O "$temp_dir/signal-cli.tar.gz" "$signal_cli_url"
        tar -xzf "$temp_dir/signal-cli.tar.gz" -C "$signal_cli_dir" --strip-components=1
        
        # Create symlink
        ln -sf "$signal_cli_dir/bin/signal-cli" "$signal_cli_bin"
        
        rm -rf "$temp_dir"
        success "signal-cli installed"
    fi
    
    # Create socket directory
    local signal_cli_socket="/var/run/signal-cli"
    mkdir -p "$signal_cli_socket"
    chown pisovereign:pisovereign "$signal_cli_socket" 2>/dev/null || true
    
    # Create systemd service for signal-cli daemon
    local signal_cli_service="/etc/systemd/system/signal-cli.service"
    
    if [[ ! -f "$signal_cli_service" ]]; then
        info "Creating signal-cli systemd service..."
        
        cat > "$signal_cli_service" << EOF
[Unit]
Description=signal-cli JSON-RPC daemon
After=network.target

[Service]
Type=simple
User=pisovereign
Group=pisovereign
ExecStart=/usr/bin/java -jar ${signal_cli_dir}/lib/signal-cli-${signal_cli_version}.jar --verbose daemon --socket ${signal_cli_socket}/socket
Restart=on-failure
RestartSec=10
RuntimeDirectory=signal-cli
RuntimeDirectoryMode=0755

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
PrivateTmp=true
ReadWritePaths=/var/run/signal-cli /var/lib/signal-cli

[Install]
WantedBy=multi-user.target
EOF
        
        # Create data directory
        mkdir -p /var/lib/signal-cli
        chown pisovereign:pisovereign /var/lib/signal-cli
        
        systemctl daemon-reload
        success "signal-cli systemd service created"
    else
        success "signal-cli systemd service already exists"
    fi
    
    echo
    warn "Signal account registration is required!"
    echo -e "${YELLOW}To register your phone number with Signal:${NC}"
    echo "  1. Run: sudo -u pisovereign signal-cli -a +YOUR_PHONE_NUMBER register"
    echo "  2. Enter the verification code: sudo -u pisovereign signal-cli -a +YOUR_PHONE_NUMBER verify CODE"
    echo "  3. Enable the daemon: systemctl enable --now signal-cli"
    echo
    echo -e "${CYAN}For more details, see: https://github.com/AsamK/signal-cli${NC}"
    echo
}

# =============================================================================
# Native Build (Rust)
# =============================================================================

install_rust() {
    step "Installing Rust toolchain"
    
    # Install as the pisovereign user or current user
    local rust_user="${SUDO_USER:-root}"
    local rust_home
    
    if [[ "$rust_user" == "root" ]]; then
        rust_home="/root"
    else
        rust_home=$(eval echo "~$rust_user")
    fi
    
    local cargo_bin="$rust_home/.cargo/bin"
    
    if [[ -f "$cargo_bin/cargo" ]]; then
        local current_version
        current_version=$("$cargo_bin/cargo" --version 2>/dev/null | awk '{print $2}' || echo "0")
        success "Rust already installed (version $current_version)"
        
        # Update if needed
        info "Updating Rust toolchain..."
        sudo -u "$rust_user" "$cargo_bin/rustup" update stable || true
    else
        info "Installing Rust via rustup..."
        
        # Download and run rustup installer
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
            sudo -u "$rust_user" sh -s -- -y --default-toolchain stable --profile minimal
        
        success "Rust installed"
    fi
    
    # Ensure cargo is in PATH for this script
    export PATH="$cargo_bin:$PATH"
    
    # Verify installation
    if command -v cargo &>/dev/null; then
        success "Cargo available: $(cargo --version)"
    else
        error "Cargo not found in PATH after installation"
        exit 1
    fi
}

clone_or_update_repo() {
    step "Fetching PiSovereign source code"
    
    local src_dir="$PISOVEREIGN_DIR/src"
    
    if [[ -d "$src_dir/.git" ]]; then
        info "Updating existing repository..."
        cd "$src_dir"
        git fetch origin
        git checkout "$PISOVEREIGN_BRANCH"
        git pull origin "$PISOVEREIGN_BRANCH"
        success "Repository updated to branch: $PISOVEREIGN_BRANCH"
    else
        info "Cloning repository..."
        rm -rf "$src_dir"
        mkdir -p "$src_dir"
        git clone --branch "$PISOVEREIGN_BRANCH" --depth 1 "$PISOVEREIGN_REPO" "$src_dir"
        success "Repository cloned (branch: $PISOVEREIGN_BRANCH)"
    fi
}

build_pisovereign() {
    step "Building PiSovereign from source"
    
    local src_dir="$PISOVEREIGN_DIR/src"
    cd "$src_dir"
    
    info "This may take 10-30 minutes on Raspberry Pi..."
    
    # Set optimized build flags for ARM64
    export RUSTFLAGS="-C target-cpu=native -C opt-level=3"
    
    # Build in release mode
    info "Building release binaries..."
    cargo build --release --bin pisovereign-server --bin pisovereign-cli 2>&1 | \
        while IFS= read -r line; do
            # Show only important lines
            if [[ "$line" == *"Compiling"* ]] || [[ "$line" == *"Finished"* ]] || [[ "$line" == *"error"* ]]; then
                echo "  $line"
            fi
        done
    
    if [[ -f "target/release/pisovereign-server" ]] && [[ -f "target/release/pisovereign-cli" ]]; then
        success "Build completed successfully"
    else
        error "Build failed - binaries not found"
        exit 1
    fi
}

install_native_binaries() {
    step "Installing native binaries"
    
    local src_dir="$PISOVEREIGN_DIR/src"
    local bin_dir="/usr/local/bin"
    
    # Stop existing service if running
    systemctl stop pisovereign.service 2>/dev/null || true
    
    # Install binaries
    info "Installing pisovereign-server..."
    install -m 755 "$src_dir/target/release/pisovereign-server" "$bin_dir/"
    
    info "Installing pisovereign-cli..."
    install -m 755 "$src_dir/target/release/pisovereign-cli" "$bin_dir/"
    
    # Verify installation
    if "$bin_dir/pisovereign-server" --version &>/dev/null; then
        success "Binaries installed: $($bin_dir/pisovereign-server --version)"
    else
        # Try without --version flag
        success "Binaries installed to $bin_dir"
    fi
    
    # Set capabilities for binding to port 443 without root
    setcap 'cap_net_bind_service=+ep' "$bin_dir/pisovereign-server" 2>/dev/null || true
}

download_prebuilt_binaries() {
    step "Downloading pre-built binaries"
    
    local arch
    arch=$(uname -m)
    
    if [[ "$arch" != "aarch64" ]]; then
        error "Pre-built binaries only available for aarch64. Use --native to build from source."
        exit 1
    fi
    
    local release_url="https://github.com/twohreichel/PiSovereign/releases"
    local version="$PISOVEREIGN_VERSION"
    
    if [[ "$version" == "latest" ]]; then
        info "Fetching latest release version..."
        version=$(curl -sL "$release_url/latest" | grep -oP 'tag/v\K[0-9.]+' | head -1) || true
        if [[ -z "$version" ]]; then
            error "Could not determine latest version. Use --native to build from source."
            exit 1
        fi
        info "Latest version: $version"
    fi
    
    local download_url="$release_url/download/v$version/pisovereign-linux-aarch64.tar.gz"
    local tmp_file="/tmp/pisovereign-binaries.tar.gz"
    
    info "Downloading from: $download_url"
    if ! curl -fsSL -o "$tmp_file" "$download_url"; then
        error "Download failed. The release may not have pre-built binaries."
        error "Use: sudo $0 --native  to build from source instead."
        exit 1
    fi
    
    # Extract binaries
    tar -xzf "$tmp_file" -C /usr/local/bin/
    rm -f "$tmp_file"
    
    chmod +x /usr/local/bin/pisovereign-server /usr/local/bin/pisovereign-cli
    
    success "Pre-built binaries installed"
}

setup_systemd_service() {
    step "Setting up systemd service"
    
    # Create systemd service file
    cat > /etc/systemd/system/pisovereign.service << 'EOF'
[Unit]
Description=PiSovereign AI Assistant
Documentation=https://twohreichel.github.io/PiSovereign/
After=network-online.target ollama.service
Wants=network-online.target
Requires=ollama.service

[Service]
Type=simple
User=pisovereign
Group=pisovereign
WorkingDirectory=/var/lib/pisovereign

# Environment
Environment="PISOVEREIGN_CONFIG=/etc/pisovereign/config.toml"
Environment="PISOVEREIGN_DATA_DIR=/var/lib/pisovereign"
Environment="RUST_LOG=info,tower_http=info"

# Binary
ExecStart=/usr/local/bin/pisovereign-server
ExecReload=/bin/kill -HUP $MAINPID

# Restart policy
Restart=on-failure
RestartSec=5
StartLimitIntervalSec=60
StartLimitBurst=3

# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
PrivateDevices=yes
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectControlGroups=yes
RestrictRealtime=yes
RestrictSUIDSGID=yes
LockPersonality=yes

# Allow write to data and log directories
ReadWritePaths=/var/lib/pisovereign /var/log/pisovereign

# Resource limits
MemoryMax=512M
CPUQuota=80%

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=pisovereign

[Install]
WantedBy=multi-user.target
EOF

    # Create Ollama service dependency (if using system Ollama)
    mkdir -p /etc/systemd/system/ollama.service.d/
    cat > /etc/systemd/system/ollama.service.d/override.conf << 'EOF'
[Service]
# Ensure Ollama stays running
Restart=always
RestartSec=3
EOF

    # Reload systemd
    systemctl daemon-reload
    
    # Enable service
    systemctl enable pisovereign.service
    
    success "Systemd service configured"
}

start_native_service() {
    step "Starting PiSovereign service"
    
    # Ensure Ollama is running first
    if systemctl is-active --quiet ollama; then
        success "Ollama service is running"
    else
        info "Starting Ollama service..."
        systemctl start ollama
        sleep 3
    fi
    
    # Start PiSovereign
    systemctl start pisovereign.service
    
    # Wait and check
    sleep 3
    
    if systemctl is-active --quiet pisovereign; then
        success "PiSovereign service started"
        
        # Show status
        local status
        status=$(systemctl status pisovereign.service --no-pager -l 2>&1 | head -10)
        echo "$status"
    else
        error "Service failed to start"
        journalctl -u pisovereign.service --no-pager -n 20
        exit 1
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

    cp "$PISOVEREIGN_DIR/config.toml.example" "$PISOVEREIGN_CONFIG_DIR/config.toml" 2>/dev/null || true
    
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

    # Baïkal CalDAV server (optional)
    local caldav_url=""
    local caldav_user=""
    local caldav_pass=""
    echo
    echo -e "${PURPLE}--- Baïkal CalDAV Server (optional) ---${NC}"
    if [[ "$INSTALL_BAIKAL" != "true" ]]; then
        if prompt_yes_no "Install Baïkal CalDAV server via Docker?"; then
            INSTALL_BAIKAL=true
        fi
    fi

    if [[ "$INSTALL_BAIKAL" == "true" ]]; then
        info "Baïkal will be deployed as a Docker container on port 5232"
        info "CalDAV URL will be set to http://baikal:80/dav.php (Docker internal)"
        caldav_url="http://baikal:80/dav.php"
        caldav_user=$(prompt_input "CalDAV username (create this user in Baïkal wizard)")
        caldav_pass=$(prompt_secret "CalDAV password (set this in Baïkal wizard)")
    else
        # External CalDAV server configuration (optional)
        echo -e "\n${PURPLE}--- CalDAV Calendar (optional) ---${NC}"
        if prompt_yes_no "Configure external CalDAV integration?"; then
            caldav_url=$(prompt_input "CalDAV server URL")
            caldav_user=$(prompt_input "CalDAV username")
            caldav_pass=$(prompt_secret "CalDAV password")
        fi
    fi
    
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
EOF

    # Add OpenTelemetry OTLP configuration if OTel Collector is enabled
    if [[ "$INSTALL_OTEL" == "true" ]]; then
        # Update telemetry section with OTLP endpoint
        if [[ "$DEPLOY_MODE" == "docker" ]]; then
            cat >> "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF

[telemetry.otlp]
endpoint = "http://otel-collector:4317"
protocol = "grpc"
EOF
        else
            cat >> "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF

[telemetry.otlp]
endpoint = "http://localhost:4317"
protocol = "grpc"
EOF
        fi
    fi

    cat >> "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF

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

    # Add CalDAV configuration
    if [[ -n "$caldav_url" ]]; then
        cat >> "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF

[caldav]
server_url = "$caldav_url"
username = "$caldav_user"
password = "$caldav_pass"
verify_certs = true
timeout_secs = 30
EOF
    fi

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
# Monitoring Stack Setup (Prometheus + Grafana)
# =============================================================================

setup_monitoring_native() {
    if [[ "$INSTALL_MONITORING" != "true" ]]; then
        return 0
    fi
    
    step "Setting up Monitoring Stack (Native - Prometheus + Grafana)"
    
    # Install Prometheus
    info "Installing Prometheus..."
    apt-get update
    apt-get install -y prometheus
    
    # Install Grafana
    info "Installing Grafana..."
    if ! grep -q "grafana" /etc/apt/sources.list.d/grafana.list 2>/dev/null; then
        apt-get install -y apt-transport-https software-properties-common
        curl -fsSL https://apt.grafana.com/gpg.key | gpg --dearmor -o /usr/share/keyrings/grafana.gpg
        echo "deb [signed-by=/usr/share/keyrings/grafana.gpg] https://apt.grafana.com stable main" | tee /etc/apt/sources.list.d/grafana.list
        apt-get update
    fi
    apt-get install -y grafana
    
    # Get the source directory (where script is located)
    local script_dir
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    local repo_grafana_dir="$(dirname "$script_dir")/grafana"
    
    # Configure Prometheus
    info "Configuring Prometheus..."
    mkdir -p /etc/prometheus/rules
    
    if [[ -f "$repo_grafana_dir/prometheus.yml" ]]; then
        cp "$repo_grafana_dir/prometheus.yml" /etc/prometheus/prometheus.yml
        info "Copied prometheus.yml from repository"
    else
        cat > /etc/prometheus/prometheus.yml << 'PROMEOF'
global:
  scrape_interval: 15s
  evaluation_interval: 15s

rule_files:
  - "/etc/prometheus/rules/*.yml"

scrape_configs:
  - job_name: "pisovereign"
    static_configs:
      - targets: ["localhost:8080"]
    metrics_path: "/metrics/prometheus"

  - job_name: "prometheus"
    static_configs:
      - targets: ["localhost:9090"]
PROMEOF
    fi
    
    # Copy alerting rules
    if [[ -f "$repo_grafana_dir/alerting_rules.yml" ]]; then
        cp "$repo_grafana_dir/alerting_rules.yml" /etc/prometheus/rules/
        info "Copied alerting_rules.yml"
    fi
    
    # Configure Grafana provisioning
    info "Configuring Grafana datasources and dashboards..."
    mkdir -p /etc/grafana/provisioning/{datasources,dashboards}
    mkdir -p /var/lib/grafana/dashboards
    
    # Datasource configuration
    if [[ -f "$repo_grafana_dir/datasources.yml" ]]; then
        # Adjust for native install (localhost instead of prometheus container name)
        sed 's|http://prometheus:9090|http://localhost:9090|g' \
            "$repo_grafana_dir/datasources.yml" > /etc/grafana/provisioning/datasources/pisovereign.yml
    else
        cat > /etc/grafana/provisioning/datasources/pisovereign.yml << 'DSEOF'
apiVersion: 1
datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://localhost:9090
    isDefault: true
    editable: false
DSEOF
    fi
    
    # Dashboard provisioning config
    cat > /etc/grafana/provisioning/dashboards/pisovereign.yml << 'DBEOF'
apiVersion: 1
providers:
  - name: 'PiSovereign'
    orgId: 1
    folder: 'PiSovereign'
    type: file
    disableDeletion: false
    updateIntervalSeconds: 30
    options:
      path: /var/lib/grafana/dashboards
DBEOF
    
    # Copy dashboard JSON
    if [[ -f "$repo_grafana_dir/dashboards/pisovereign.json" ]]; then
        cp "$repo_grafana_dir/dashboards/pisovereign.json" /var/lib/grafana/dashboards/
        chown grafana:grafana /var/lib/grafana/dashboards/pisovereign.json
        info "Copied PiSovereign dashboard"
    fi
    
    # Set Grafana admin password
    local grafana_password="${GRAFANA_ADMIN_PASSWORD:-pisovereign}"
    sed -i "s/;admin_password = admin/admin_password = $grafana_password/" /etc/grafana/grafana.ini 2>/dev/null || true
    
    # Set ownership
    chown -R prometheus:prometheus /etc/prometheus
    chown -R grafana:grafana /etc/grafana/provisioning
    
    # Enable and start services
    systemctl daemon-reload
    systemctl enable prometheus grafana-server
    systemctl restart prometheus grafana-server
    
    # Add firewall rules if UFW is active
    if command -v ufw &>/dev/null && ufw status | grep -q "Status: active"; then
        ufw allow from 127.0.0.1 to any port 9090 proto tcp comment 'Prometheus (local only)'
        ufw allow from 127.0.0.1 to any port 3001 proto tcp comment 'Grafana (local only)'
        info "Added firewall rules for Prometheus and Grafana (localhost only)"
    fi
    
    success "Monitoring stack installed (Native)"
    info "Prometheus: http://localhost:9090"
    info "Grafana: http://localhost:3000 (admin/$grafana_password)"
}

setup_monitoring_docker() {
    if [[ "$INSTALL_MONITORING" != "true" ]]; then
        return 0
    fi
    
    step "Setting up Monitoring Stack (Docker - Prometheus + Grafana)"
    
    # Create monitoring directories
    mkdir -p "$PISOVEREIGN_DIR/grafana"/{dashboards,provisioning/datasources,provisioning/dashboards}
    mkdir -p "$PISOVEREIGN_DIR/prometheus"
    
    # Get the source directory
    local script_dir
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    local repo_grafana_dir="$(dirname "$script_dir")/grafana"
    
    # Copy Prometheus config (adjust for Docker networking)
    if [[ -f "$repo_grafana_dir/prometheus.yml" ]]; then
        sed 's/localhost:8080/pisovereign:8080/g; s/localhost:9090/prometheus:9090/g' \
            "$repo_grafana_dir/prometheus.yml" > "$PISOVEREIGN_DIR/prometheus/prometheus.yml"
        info "Copied prometheus.yml (adjusted for Docker networking)"
    else
        cat > "$PISOVEREIGN_DIR/prometheus/prometheus.yml" << 'PROMEOF'
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: "pisovereign"
    static_configs:
      - targets: ["pisovereign:8080"]
    metrics_path: "/metrics/prometheus"

  - job_name: "prometheus"
    static_configs:
      - targets: ["localhost:9090"]
PROMEOF
    fi
    
    # Copy alerting rules
    if [[ -f "$repo_grafana_dir/alerting_rules.yml" ]]; then
        cp "$repo_grafana_dir/alerting_rules.yml" "$PISOVEREIGN_DIR/prometheus/"
        info "Copied alerting_rules.yml"
    fi
    
    # Copy Grafana datasources
    if [[ -f "$repo_grafana_dir/datasources.yml" ]]; then
        cp "$repo_grafana_dir/datasources.yml" "$PISOVEREIGN_DIR/grafana/provisioning/datasources/"
    else
        cat > "$PISOVEREIGN_DIR/grafana/provisioning/datasources/datasources.yml" << 'DSEOF'
apiVersion: 1
datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://prometheus:9090
    isDefault: true
    editable: false
DSEOF
    fi
    
    # Dashboard provisioning config
    cat > "$PISOVEREIGN_DIR/grafana/provisioning/dashboards/dashboards.yml" << 'DBEOF'
apiVersion: 1
providers:
  - name: 'PiSovereign'
    orgId: 1
    folder: 'PiSovereign'
    type: file
    disableDeletion: false
    updateIntervalSeconds: 30
    options:
      path: /etc/grafana/provisioning/dashboards
DBEOF
    
    # Copy dashboard JSON
    if [[ -f "$repo_grafana_dir/dashboards/pisovereign.json" ]]; then
        cp "$repo_grafana_dir/dashboards/pisovereign.json" "$PISOVEREIGN_DIR/grafana/dashboards/"
        info "Copied PiSovereign dashboard"
    fi
    
    success "Monitoring configuration prepared for Docker"
}

# =============================================================================
# OpenTelemetry Collector Setup
# =============================================================================

setup_otel_native() {
    if [[ "$INSTALL_OTEL" != "true" ]]; then
        return 0
    fi

    step "Setting up OpenTelemetry Collector (Native - Docker container)"

    # Even in native mode, run the OTel Collector as a Docker container
    # for easier management and updates
    if ! command -v docker &>/dev/null; then
        info "Installing Docker for OpenTelemetry Collector..."
        install_docker
    fi

    mkdir -p "$PISOVEREIGN_DIR/otel"

    # Create OTel Collector config
    cat > "$PISOVEREIGN_DIR/otel/otel-collector-config.yaml" << 'OTELEOF'
# OpenTelemetry Collector Configuration for PiSovereign
# =====================================================

receivers:
  otlp:
    protocols:
      grpc:
        endpoint: "0.0.0.0:4317"
      http:
        endpoint: "0.0.0.0:4318"

processors:
  batch:
    send_batch_size: 1024
    timeout: 5s
  memory_limiter:
    check_interval: 1s
    limit_mib: 256
    spike_limit_mib: 64
  resource:
    attributes:
      - key: service.namespace
        value: pisovereign
        action: upsert

exporters:
  debug:
    verbosity: basic

extensions:
  health_check:
    endpoint: "0.0.0.0:13133"
  zpages:
    endpoint: "0.0.0.0:55679"

service:
  extensions: [health_check, zpages]
  pipelines:
    traces:
      receivers: [otlp]
      processors: [memory_limiter, resource, batch]
      exporters: [debug]
    metrics:
      receivers: [otlp]
      processors: [memory_limiter, resource, batch]
      exporters: [debug]
    logs:
      receivers: [otlp]
      processors: [memory_limiter, resource, batch]
      exporters: [debug]
OTELEOF

    # If monitoring (Prometheus) is also enabled, add the Prometheus exporter
    if [[ "$INSTALL_MONITORING" == "true" ]]; then
        cat > "$PISOVEREIGN_DIR/otel/otel-collector-config.yaml" << 'OTELEOF'
# OpenTelemetry Collector Configuration for PiSovereign
# =====================================================

receivers:
  otlp:
    protocols:
      grpc:
        endpoint: "0.0.0.0:4317"
      http:
        endpoint: "0.0.0.0:4318"

processors:
  batch:
    send_batch_size: 1024
    timeout: 5s
  memory_limiter:
    check_interval: 1s
    limit_mib: 256
    spike_limit_mib: 64
  resource:
    attributes:
      - key: service.namespace
        value: pisovereign
        action: upsert

exporters:
  debug:
    verbosity: basic
  otlphttp/prometheus:
    endpoint: "http://localhost:9090/api/v1/otlp"
    tls:
      insecure: true

extensions:
  health_check:
    endpoint: "0.0.0.0:13133"
  zpages:
    endpoint: "0.0.0.0:55679"

service:
  extensions: [health_check, zpages]
  pipelines:
    traces:
      receivers: [otlp]
      processors: [memory_limiter, resource, batch]
      exporters: [debug]
    metrics:
      receivers: [otlp]
      processors: [memory_limiter, resource, batch]
      exporters: [debug, otlphttp/prometheus]
    logs:
      receivers: [otlp]
      processors: [memory_limiter, resource, batch]
      exporters: [debug]
OTELEOF
    fi

    # Create systemd service for OTel Collector (via Docker)
    cat > /etc/systemd/system/otel-collector.service << EOF
[Unit]
Description=OpenTelemetry Collector (Docker)
After=network-online.target docker.service
Wants=network-online.target
Requires=docker.service

[Service]
Type=simple
Restart=on-failure
RestartSec=10
ExecStartPre=-/usr/bin/docker rm -f otel-collector
ExecStart=/usr/bin/docker run --rm --name otel-collector \\
    --network host \\
    -v $PISOVEREIGN_DIR/otel/otel-collector-config.yaml:/etc/otelcol-contrib/config.yaml:ro \\
    otel/opentelemetry-collector-contrib:latest
ExecStop=/usr/bin/docker stop otel-collector

[Install]
WantedBy=multi-user.target
EOF

    systemctl daemon-reload
    systemctl enable otel-collector.service

    # Pull the image
    info "Pulling OpenTelemetry Collector Docker image..."
    docker pull otel/opentelemetry-collector-contrib:latest

    # Start the service
    systemctl start otel-collector.service

    # Add firewall rules if UFW is active
    if command -v ufw &>/dev/null && ufw status | grep -q "Status: active"; then
        ufw allow from 127.0.0.1 to any port 4317 proto tcp comment 'OTel gRPC (local only)'
        ufw allow from 127.0.0.1 to any port 4318 proto tcp comment 'OTel HTTP (local only)'
        ufw allow from 127.0.0.1 to any port 13133 proto tcp comment 'OTel Health (local only)'
        info "Added firewall rules for OpenTelemetry Collector (localhost only)"
    fi

    success "OpenTelemetry Collector installed (Native via Docker)"
    info "OTLP gRPC: http://localhost:4317"
    info "OTLP HTTP: http://localhost:4318"
    info "Health:    http://localhost:13133"
}

setup_otel_docker() {
    if [[ "$INSTALL_OTEL" != "true" ]]; then
        return 0
    fi

    step "Setting up OpenTelemetry Collector (Docker)"

    mkdir -p "$PISOVEREIGN_DIR/otel"

    # Create OTel Collector config
    cat > "$PISOVEREIGN_DIR/otel/otel-collector-config.yaml" << 'OTELEOF'
# OpenTelemetry Collector Configuration for PiSovereign
# =====================================================

receivers:
  otlp:
    protocols:
      grpc:
        endpoint: "0.0.0.0:4317"
      http:
        endpoint: "0.0.0.0:4318"

processors:
  batch:
    send_batch_size: 1024
    timeout: 5s
  memory_limiter:
    check_interval: 1s
    limit_mib: 256
    spike_limit_mib: 64
  resource:
    attributes:
      - key: service.namespace
        value: pisovereign
        action: upsert

exporters:
  debug:
    verbosity: basic

extensions:
  health_check:
    endpoint: "0.0.0.0:13133"
  zpages:
    endpoint: "0.0.0.0:55679"

service:
  extensions: [health_check, zpages]
  pipelines:
    traces:
      receivers: [otlp]
      processors: [memory_limiter, resource, batch]
      exporters: [debug]
    metrics:
      receivers: [otlp]
      processors: [memory_limiter, resource, batch]
      exporters: [debug]
    logs:
      receivers: [otlp]
      processors: [memory_limiter, resource, batch]
      exporters: [debug]
OTELEOF

    # If monitoring (Prometheus) is also enabled, add the Prometheus exporter
    if [[ "$INSTALL_MONITORING" == "true" ]]; then
        cat > "$PISOVEREIGN_DIR/otel/otel-collector-config.yaml" << 'OTELEOF'
# OpenTelemetry Collector Configuration for PiSovereign
# =====================================================

receivers:
  otlp:
    protocols:
      grpc:
        endpoint: "0.0.0.0:4317"
      http:
        endpoint: "0.0.0.0:4318"

processors:
  batch:
    send_batch_size: 1024
    timeout: 5s
  memory_limiter:
    check_interval: 1s
    limit_mib: 256
    spike_limit_mib: 64
  resource:
    attributes:
      - key: service.namespace
        value: pisovereign
        action: upsert

exporters:
  debug:
    verbosity: basic
  otlphttp/prometheus:
    endpoint: "http://prometheus:9090/api/v1/otlp"
    tls:
      insecure: true

extensions:
  health_check:
    endpoint: "0.0.0.0:13133"
  zpages:
    endpoint: "0.0.0.0:55679"

service:
  extensions: [health_check, zpages]
  pipelines:
    traces:
      receivers: [otlp]
      processors: [memory_limiter, resource, batch]
      exporters: [debug]
    metrics:
      receivers: [otlp]
      processors: [memory_limiter, resource, batch]
      exporters: [debug, otlphttp/prometheus]
    logs:
      receivers: [otlp]
      processors: [memory_limiter, resource, batch]
      exporters: [debug]
OTELEOF
        info "Configured OTel Collector with Prometheus OTLP export"
    fi

    success "OpenTelemetry Collector configuration prepared for Docker"
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
    
    # Setup monitoring configs first if enabled
    if [[ "$INSTALL_MONITORING" == "true" ]]; then
        setup_monitoring_docker
    fi
    
    # Setup OpenTelemetry Collector configs if enabled
    if [[ "$INSTALL_OTEL" == "true" ]]; then
        setup_otel_docker
    fi
    
    # Create docker-compose.yml - services section first
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
EOF

    # Add OTEL environment variables if enabled
    if [[ "$INSTALL_OTEL" == "true" ]]; then
        cat >> docker-compose.yml << 'EOF'
      - OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector:4317
      - OTEL_SERVICE_NAME=pisovereign
EOF
    fi

    cat >> docker-compose.yml << 'EOF'
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
EOF

    # Add monitoring services if enabled (within services section)
    if [[ "$INSTALL_MONITORING" == "true" ]]; then
        cat >> docker-compose.yml << 'MONEOF'

  prometheus:
    image: prom/prometheus:latest
    container_name: prometheus
    restart: unless-stopped
    ports:
      - "127.0.0.1:9090:9090"
    volumes:
      - ./prometheus/prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - ./prometheus/alerting_rules.yml:/etc/prometheus/rules/alerting_rules.yml:ro
      - prometheus_data:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
      - '--storage.tsdb.retention.time=7d'
      - '--storage.tsdb.retention.size=1GB'
      - '--web.enable-lifecycle'
    networks:
      - pisovereign-net
    healthcheck:
      test: ["CMD", "wget", "-q", "--spider", "http://localhost:9090/-/healthy"]
      interval: 30s
      timeout: 10s
      retries: 3

  grafana:
    image: grafana/grafana:latest
    container_name: grafana
    restart: unless-stopped
    ports:
      - "127.0.0.1:3001:3000"
    volumes:
      - ./grafana/provisioning/datasources:/etc/grafana/provisioning/datasources:ro
      - ./grafana/provisioning/dashboards:/etc/grafana/provisioning/dashboards:ro
      - ./grafana/dashboards:/var/lib/grafana/dashboards:ro
      - grafana_data:/var/lib/grafana
    environment:
      - GF_SECURITY_ADMIN_USER=admin
      - GF_SECURITY_ADMIN_PASSWORD=${GRAFANA_ADMIN_PASSWORD:-pisovereign}
      - GF_USERS_ALLOW_SIGN_UP=false
      - GF_SERVER_ROOT_URL=http://localhost:3001
    depends_on:
      - prometheus
    networks:
      - pisovereign-net
    healthcheck:
      test: ["CMD", "wget", "-q", "--spider", "http://localhost:3000/api/health"]
      interval: 30s
      timeout: 10s
      retries: 3
MONEOF
        info "Added Prometheus and Grafana services"
    fi

    # Add Baïkal CalDAV service if enabled
    if [[ "$INSTALL_BAIKAL" == "true" ]]; then
        cat >> docker-compose.yml << 'EOF'

  baikal:
    image: ckulka/baikal:nginx
    container_name: baikal
    restart: unless-stopped
    ports:
      - "127.0.0.1:5232:80"
    volumes:
      - baikal-config:/var/www/baikal/config
      - baikal-data:/var/www/baikal/Specific
    networks:
      - pisovereign-net
    healthcheck:
      test: ["CMD", "wget", "-q", "--spider", "http://localhost:80/dav.php"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 10s
EOF
        info "Added Baïkal CalDAV service"
    fi

    # Add OpenTelemetry Collector service if enabled
    if [[ "$INSTALL_OTEL" == "true" ]]; then
        cat >> docker-compose.yml << 'EOF'

  otel-collector:
    image: otel/opentelemetry-collector-contrib:latest
    container_name: otel-collector
    restart: unless-stopped
    ports:
      - "127.0.0.1:4317:4317"   # OTLP gRPC
      - "127.0.0.1:4318:4318"   # OTLP HTTP
      - "127.0.0.1:13133:13133" # Health check
      - "127.0.0.1:55679:55679" # zPages
    volumes:
      - ./otel/otel-collector-config.yaml:/etc/otelcol-contrib/config.yaml:ro
    networks:
      - pisovereign-net
    healthcheck:
      test: ["CMD", "wget", "-q", "--spider", "http://localhost:13133"]
      interval: 30s
      timeout: 10s
      retries: 3
EOF
        info "Added OpenTelemetry Collector service"
    fi

    # Add networks section
    cat >> docker-compose.yml << 'EOF'

networks:
  pisovereign-net:
    driver: bridge
EOF

    # Add volumes section (conditionally include monitoring and baikal volumes)
    {
        echo ""
        echo "volumes:"
        echo "  ollama-models:"
        if [[ "$INSTALL_MONITORING" == "true" ]]; then
            echo "  prometheus_data:"
            echo "  grafana_data:"
        fi
        if [[ "$INSTALL_BAIKAL" == "true" ]]; then
            echo "  baikal-config:"
            echo "  baikal-data:"
        fi
    } >> docker-compose.yml

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

setup_native_auto_update() {
    step "Setting up automatic updates (native mode)"
    
    mkdir -p "$PISOVEREIGN_DIR/scripts"
    
    # Create update script for native deployment
    cat > "$PISOVEREIGN_DIR/scripts/auto-update.sh" << 'UPDATEEOF'
#!/usr/bin/env bash
# PiSovereign Auto-Update Script (Native Mode)

set -euo pipefail

LOG_FILE="/var/log/pisovereign/auto-update.log"
PISOVEREIGN_DIR="/opt/pisovereign"
SRC_DIR="$PISOVEREIGN_DIR/src"

log() {
    echo "[$(date -Iseconds)] $*" | tee -a "$LOG_FILE"
}

log "=== Starting auto-update (native mode) ==="

# Update system packages
log "Updating system packages..."
apt-get update -qq
DEBIAN_FRONTEND=noninteractive apt-get upgrade -y -qq

# Check for PiSovereign updates
log "Checking for PiSovereign updates..."
cd "$SRC_DIR"

# Fetch latest changes
git fetch origin main

# Check if there are updates
LOCAL=$(git rev-parse HEAD)
REMOTE=$(git rev-parse origin/main)

if [[ "$LOCAL" != "$REMOTE" ]]; then
    log "New version available, updating..."
    
    # Pull latest changes
    git pull origin main
    
    # Rebuild
    log "Rebuilding PiSovereign..."
    export RUSTFLAGS="-C target-cpu=native -C opt-level=3"
    
    if cargo build --release --bin pisovereign-server --bin pisovereign-cli 2>&1 | tee -a "$LOG_FILE"; then
        # Stop service
        systemctl stop pisovereign.service
        
        # Install new binaries
        install -m 755 target/release/pisovereign-server /usr/local/bin/
        install -m 755 target/release/pisovereign-cli /usr/local/bin/
        
        # Restart service
        systemctl start pisovereign.service
        
        log "PiSovereign updated to $(git describe --tags --always)"
    else
        log "ERROR: Build failed, keeping current version"
    fi
else
    log "PiSovereign is up to date"
fi

# Update LLM model
log "Checking LLM model updates..."
ollama pull qwen2.5:1.5b 2>&1 | grep -v "up to date" || true

# Cleanup
log "Cleaning up..."
apt-get autoremove -y -qq
apt-get autoclean -qq

# Clean old Rust build artifacts (keep last 2 builds)
cd "$SRC_DIR"
cargo clean --release 2>/dev/null || true

log "=== Auto-update completed ==="
UPDATEEOF

    chmod +x "$PISOVEREIGN_DIR/scripts/auto-update.sh"
    
    # Create systemd service
    cat > /etc/systemd/system/pisovereign-update.service << 'EOF'
[Unit]
Description=PiSovereign Auto-Update (Native)
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
ExecStart=/opt/pisovereign/scripts/auto-update.sh
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

setup_docker_auto_update() {
    step "Setting up automatic updates (Docker mode)"
    
    mkdir -p "$PISOVEREIGN_DIR/scripts"
    
    # Create update script for Docker deployment
    cat > "$PISOVEREIGN_DIR/scripts/auto-update.sh" << 'UPDATEEOF'
#!/usr/bin/env bash
# PiSovereign Auto-Update Script (Docker Mode)

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
ollama pull qwen2.5:1.5b 2>&1 | grep -v "up to date" || true

# Cleanup
log "Cleaning up..."
docker system prune -f --volumes 2>/dev/null || true
apt-get autoremove -y -qq
apt-get autoclean -qq

log "Auto-update completed"
UPDATEEOF

    chmod +x "$PISOVEREIGN_DIR/scripts/auto-update.sh"
    
    # Create systemd service
    cat > /etc/systemd/system/pisovereign-update.service << 'EOF'
[Unit]
Description=PiSovereign Auto-Update (Docker)
After=network-online.target docker.service
Wants=network-online.target

[Service]
Type=oneshot
ExecStart=/opt/pisovereign/scripts/auto-update.sh
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
    if [[ "$DEPLOY_MODE" == "native" ]]; then
        # Native mode - restart service to reopen log files
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
        systemctl reload pisovereign.service 2>/dev/null || systemctl restart pisovereign.service 2>/dev/null || true
    endscript
}
EOF
    else
        # Docker mode - send signal to container
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
    fi
    
    success "Log rotation configured"
}

# =============================================================================
# Verification
# =============================================================================

verify_installation() {
    step "Verifying installation"
    
    local errors=0
    
    # Check Docker (only in Docker mode)
    if [[ "$DEPLOY_MODE" == "docker" ]]; then
        if docker info &>/dev/null; then
            success "Docker: OK"
        else
            error "Docker: FAILED"
            ((errors++))
        fi
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
    
    # Check PiSovereign service/container
    if [[ "$DEPLOY_MODE" == "native" ]]; then
        if systemctl is-active --quiet pisovereign.service; then
            success "PiSovereign service: OK"
        else
            error "PiSovereign service: FAILED"
            ((errors++))
        fi
    fi
    
    # Check PiSovereign API
    sleep 2  # Give service time to start
    if curl -sf http://localhost:3000/health &>/dev/null; then
        success "PiSovereign API: OK"
    else
        warn "PiSovereign API: Not responding yet (may still be starting)"
    fi
    
    # Check Monitoring Stack (if installed)
    if [[ "$INSTALL_MONITORING" == "true" ]]; then
        if curl -sf http://localhost:9090/-/healthy &>/dev/null; then
            success "Prometheus: OK"
        else
            warn "Prometheus: Starting..."
        fi
        
        local grafana_port=3000
        [[ "$DEPLOY_MODE" == "docker" ]] && grafana_port=3001
        if curl -sf "http://localhost:$grafana_port/api/health" &>/dev/null; then
            success "Grafana: OK"
        else
            warn "Grafana: Starting..."
        fi
    fi
    
    # Check OpenTelemetry Collector (if installed)
    if [[ "$INSTALL_OTEL" == "true" ]]; then
        if curl -sf http://localhost:13133 &>/dev/null; then
            success "OpenTelemetry Collector: OK"
        else
            warn "OpenTelemetry Collector: Starting..."
        fi
    fi
    
    # Check auto-update timer
    if systemctl is-active pisovereign-update.timer &>/dev/null; then
        success "Auto-update timer: OK"
    else
        warn "Auto-update timer: NOT ACTIVE"
    fi
    
    # Check binaries in native mode
    if [[ "$DEPLOY_MODE" == "native" ]]; then
        if [[ -x /usr/local/bin/pisovereign-server ]]; then
            success "Binaries installed: OK"
        else
            error "Binaries: NOT FOUND"
            ((errors++))
        fi
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
    echo -e "${CYAN}Deployment Mode:${NC} $DEPLOY_MODE"
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
    
    if [[ "$INSTALL_MONITORING" == "true" ]]; then
        local grafana_port=3000
        local grafana_pass="${GRAFANA_ADMIN_PASSWORD:-pisovereign}"
        [[ "$DEPLOY_MODE" == "docker" ]] && grafana_port=3001
        echo "  Prometheus:     http://localhost:9090"
        echo "  Grafana:        http://localhost:$grafana_port (admin/$grafana_pass)"
    fi
    
    if [[ "$INSTALL_OTEL" == "true" ]]; then
        echo "  OTel Collector: gRPC=localhost:4317, HTTP=localhost:4318"
        echo "  OTel Health:    http://localhost:13133"
        echo "  OTel zPages:    http://localhost:55679/debug/tracez"
    fi
    
    if [[ "$INSTALL_BAIKAL" == "true" ]]; then
        echo "  Baïkal CalDAV:   http://localhost:5232"
    fi
    
    if [[ -n "$domain" ]]; then
        echo "  Public URL:     https://$domain"
    fi
    
    echo
    echo -e "${CYAN}Management:${NC}"
    
    if [[ "$DEPLOY_MODE" == "native" ]]; then
        echo "  View logs:      journalctl -u pisovereign -f"
        echo "  Restart:        systemctl restart pisovereign"
        echo "  Status:         systemctl status pisovereign"
    else
        echo "  View logs:      docker compose -f $PISOVEREIGN_DIR/docker-compose.yml logs -f"
        echo "  Restart:        docker compose -f $PISOVEREIGN_DIR/docker-compose.yml restart"
    fi
    
    echo "  Update now:     $PISOVEREIGN_DIR/scripts/auto-update.sh"
    echo "  Update timer:   systemctl status pisovereign-update.timer"
    
    if [[ "$INSTALL_OTEL" == "true" ]]; then
        echo
        echo -e "${CYAN}OpenTelemetry:${NC}"
        echo "  OTLP gRPC endpoint:   http://localhost:4317"
        echo "  OTLP HTTP endpoint:   http://localhost:4318"
        echo "  Health check:         http://localhost:13133"
        echo "  Debug zPages:         http://localhost:55679/debug/tracez"
        echo "  Config:               $PISOVEREIGN_DIR/otel/otel-collector-config.yaml"
        if [[ "$DEPLOY_MODE" == "native" ]]; then
            echo "  Status:               systemctl status otel-collector"
            echo "  Logs:                 journalctl -u otel-collector -f"
        else
            echo "  Logs:                 docker compose logs otel-collector -f"
        fi
    fi
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
    
    if [[ "$INSTALL_BAIKAL" == "true" ]]; then
        echo
        echo -e "${YELLOW}Baïkal CalDAV Setup Required:${NC}"
        echo "  1. Open http://localhost:5232 in your browser"
        echo "  2. Complete the setup wizard (set admin password, choose SQLite)"
        echo "  3. Create a user matching your CalDAV username in config.toml"
        echo "  4. Create a default calendar for that user"
        echo "  5. Update calendar_path in config.toml:"
        echo "     calendar_path = \"/calendars/<USERNAME>/default/\""
    fi
    
    if [[ "$DEPLOY_MODE" == "native" ]]; then
        echo "  4. Review logs: journalctl -u pisovereign -f"
    else
        echo "  4. Review logs: docker compose logs -f"
    fi
    echo
}

# =============================================================================
# Main
# =============================================================================

main() {
    # Parse command line arguments first
    parse_args "$@"
    
    echo
    echo -e "${PURPLE}╔═══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${PURPLE}║         PiSovereign Setup Script for Raspberry Pi         ║${NC}"
    echo -e "${PURPLE}╚═══════════════════════════════════════════════════════════╝${NC}"
    echo
    
    check_root
    check_platform
    
    echo
    info "Deployment mode: ${CYAN}$DEPLOY_MODE${NC}"
    info "This script will install and configure PiSovereign on your Raspberry Pi."
    
    if [[ "$DEPLOY_MODE" == "native" ]]; then
        info "Native build will compile from source (~20-30 min on Pi 5)"
    else
        info "Docker mode will pull container images (~5-10 min)"
    fi
    
    if [[ "$SKIP_SECURITY" == "true" ]]; then
        warn "Security hardening will be SKIPPED"
    fi
    echo
    
    if ! prompt_yes_no "Continue with installation?"; then
        info "Installation cancelled"
        exit 0
    fi
    
    # Installation - base packages
    install_base_packages
    install_hailo_sdk
    install_ollama
    install_whisper_cpp
    install_piper
    install_signal_cli
    
    # User and directories (needed for both modes)
    setup_pisovereign_user
    
    # Security hardening (optional)
    if [[ "$SKIP_SECURITY" != "true" ]]; then
        setup_ssh_hardening
        setup_firewall
        setup_fail2ban
        setup_kernel_hardening
        setup_auto_updates
    else
        warn "Skipping security hardening as requested"
    fi
    
    # Configuration
    configure_toml
    
    # Deployment - based on mode
    if [[ "$DEPLOY_MODE" == "native" ]]; then
        # Native build deployment
        if [[ "$SKIP_BUILD" == "true" ]]; then
            download_prebuilt_binaries
        else
            install_rust
            clone_or_update_repo
            build_pisovereign
            install_native_binaries
        fi
        setup_systemd_service
        start_native_service
        setup_native_auto_update
        
        # Monitoring stack (Native mode)
        if [[ "$INSTALL_MONITORING" == "true" ]]; then
            setup_monitoring_native
        fi
        
        # OpenTelemetry Collector (Native mode - runs as Docker container)
        if [[ "$INSTALL_OTEL" == "true" ]]; then
            setup_otel_native
        fi
    else
        # Docker deployment
        install_docker
        setup_docker_compose
        start_services
        setup_docker_auto_update
    fi
    
    # Logrotate for both modes
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
