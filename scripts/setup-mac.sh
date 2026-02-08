#!/usr/bin/env bash
# =============================================================================
# PiSovereign - macOS Setup Script
# =============================================================================
#
# This script automates the complete setup of PiSovereign on macOS:
#   - Homebrew installation (if not present)
#   - Ollama for local LLM inference (with Metal acceleration)
#   - whisper.cpp for local speech-to-text
#   - Piper for local text-to-speech
#   - FFmpeg and other dependencies
#   - Interactive config.toml configuration
#   - Docker Compose deployment (development mode)
#   - Automatic update system via launchd
#
# Requirements:
#   - macOS 13 (Ventura) or later
#   - Apple Silicon (M1/M2/M3) or Intel Mac
#   - Docker Desktop installed
#   - Admin privileges (for some installations)
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/twohreichel/PiSovereign/main/scripts/setup-mac.sh | bash
#
# =============================================================================

set -euo pipefail

# =============================================================================
# Configuration
# =============================================================================

PISOVEREIGN_VERSION="${PISOVEREIGN_VERSION:-latest}"
PISOVEREIGN_DIR="${HOME}/.pisovereign"
PISOVEREIGN_CONFIG_DIR="${HOME}/.config/pisovereign"

WHISPER_MODEL_DIR="${HOME}/Library/Application Support/whisper/models"
WHISPER_MODEL="ggml-base.bin"
WHISPER_MODEL_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin"

PIPER_DIR="${HOME}/Library/Application Support/piper/voices"
PIPER_VOICE="de_DE-thorsten-medium"
PIPER_VOICE_URL="https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/de/de_DE/thorsten/medium"

OLLAMA_MODEL="qwen2.5:1.5b"

LAUNCH_AGENTS_DIR="${HOME}/Library/LaunchAgents"

# =============================================================================
# Colors and Output Functions
# =============================================================================

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color
BOLD='\033[1m'

info() { echo -e "${BLUE}[INFO]${NC} $*"; }
success() { echo -e "${GREEN}[✓]${NC} $*"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }
step() { echo -e "\n${PURPLE}==>${NC} ${CYAN}$*${NC}"; }

# =============================================================================
# Helper Functions
# =============================================================================

check_macos() {
    if [[ "$(uname -s)" != "Darwin" ]]; then
        error "This script is for macOS only. Detected: $(uname -s)"
        exit 1
    fi
    
    # Check macOS version (need 13+)
    local macos_version
    macos_version=$(sw_vers -productVersion | cut -d. -f1)
    if [[ "$macos_version" -lt 13 ]]; then
        error "macOS 13 (Ventura) or later required. Detected: $(sw_vers -productVersion)"
        exit 1
    fi
    
    success "macOS $(sw_vers -productVersion) detected"
}

get_arch() {
    local arch
    arch=$(uname -m)
    case "$arch" in
        arm64) echo "aarch64" ;;
        x86_64) echo "x86_64" ;;
        *) error "Unsupported architecture: $arch"; exit 1 ;;
    esac
}

prompt() {
    local var_name="$1"
    local prompt_text="$2"
    local default="${3:-}"
    local value
    
    if [[ -n "$default" ]]; then
        read -rp "$(echo -e "${CYAN}$prompt_text${NC} [$default]: ")" value
        value="${value:-$default}"
    else
        read -rp "$(echo -e "${CYAN}$prompt_text${NC}: ")" value
    fi
    
    eval "$var_name=\"\$value\""
}

prompt_secret() {
    local var_name="$1"
    local prompt_text="$2"
    local value
    
    read -rsp "$(echo -e "${CYAN}$prompt_text${NC}: ")" value
    echo
    eval "$var_name=\"\$value\""
}

prompt_yes_no() {
    local prompt_text="$1"
    local default="${2:-n}"
    local response
    
    if [[ "$default" == "y" ]]; then
        read -rp "$(echo -e "${CYAN}$prompt_text${NC} [Y/n]: ")" response
        response="${response:-y}"
    else
        read -rp "$(echo -e "${CYAN}$prompt_text${NC} [y/N]: ")" response
        response="${response:-n}"
    fi
    
    [[ "$response" =~ ^[Yy] ]]
}

command_exists() {
    command -v "$1" &>/dev/null
}

# =============================================================================
# Installation Functions
# =============================================================================

install_homebrew() {
    step "Checking Homebrew"
    
    if command_exists brew; then
        success "Homebrew already installed"
        brew update
        return
    fi
    
    info "Installing Homebrew..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    
    # Add Homebrew to PATH for Apple Silicon
    if [[ "$(uname -m)" == "arm64" ]]; then
        echo 'eval "$(/opt/homebrew/bin/brew shellenv)"' >> ~/.zprofile
        eval "$(/opt/homebrew/bin/brew shellenv)"
    fi
    
    success "Homebrew installed"
}

install_dependencies() {
    step "Installing dependencies"
    
    # Essential packages
    local packages=(
        ffmpeg
        jq
        curl
        wget
    )
    
    for pkg in "${packages[@]}"; do
        if brew list "$pkg" &>/dev/null; then
            success "$pkg already installed"
        else
            info "Installing $pkg..."
            brew install "$pkg"
            success "$pkg installed"
        fi
    done
}

check_docker() {
    step "Checking Docker Desktop"
    
    if ! command_exists docker; then
        warn "Docker Desktop not found!"
        echo
        echo -e "${YELLOW}Please install Docker Desktop manually:${NC}"
        echo "  1. Download from: https://www.docker.com/products/docker-desktop/"
        echo "  2. Install and start Docker Desktop"
        echo "  3. Re-run this script"
        echo
        
        if prompt_yes_no "Open Docker Desktop download page?"; then
            open "https://www.docker.com/products/docker-desktop/"
        fi
        
        error "Docker Desktop required - please install and re-run"
        exit 1
    fi
    
    # Check if Docker daemon is running
    if ! docker info &>/dev/null; then
        warn "Docker Desktop is installed but not running"
        info "Starting Docker Desktop..."
        open -a Docker
        
        echo "Waiting for Docker to start..."
        local max_attempts=60
        local attempt=0
        while ! docker info &>/dev/null && [[ $attempt -lt $max_attempts ]]; do
            sleep 2
            ((attempt++))
            echo -n "."
        done
        echo
        
        if docker info &>/dev/null; then
            success "Docker Desktop is running"
        else
            error "Docker Desktop failed to start - please start it manually and re-run"
            exit 1
        fi
    else
        success "Docker Desktop is running"
    fi
}

install_ollama() {
    step "Installing Ollama"
    
    if command_exists ollama; then
        success "Ollama already installed"
    else
        info "Installing Ollama via Homebrew..."
        brew install ollama
        success "Ollama installed"
    fi
    
    # Check if Ollama is running
    if ! curl -sf http://localhost:11434/api/tags &>/dev/null; then
        info "Starting Ollama service..."
        brew services start ollama
        
        # Wait for Ollama to be ready
        local max_attempts=30
        local attempt=0
        while ! curl -sf http://localhost:11434/api/tags &>/dev/null && [[ $attempt -lt $max_attempts ]]; do
            sleep 2
            ((attempt++))
        done
    fi
    
    if curl -sf http://localhost:11434/api/tags &>/dev/null; then
        success "Ollama is running"
        
        # Pull default model
        info "Pulling LLM model: $OLLAMA_MODEL (this may take a while)..."
        ollama pull "$OLLAMA_MODEL"
        success "Model $OLLAMA_MODEL ready"
    else
        error "Failed to start Ollama"
        exit 1
    fi
}

install_whisper_cpp() {
    step "Installing whisper.cpp"
    
    if command_exists whisper-cli; then
        success "whisper.cpp already installed"
    else
        info "Installing whisper.cpp via Homebrew..."
        brew install whisper-cpp
        success "whisper.cpp installed"
    fi
    
    # Download model
    mkdir -p "$WHISPER_MODEL_DIR"
    
    if [[ -f "$WHISPER_MODEL_DIR/$WHISPER_MODEL" ]]; then
        success "Whisper model already exists"
    else
        info "Downloading Whisper model: $WHISPER_MODEL..."
        curl -L --progress-bar -o "$WHISPER_MODEL_DIR/$WHISPER_MODEL" "$WHISPER_MODEL_URL"
        success "Whisper model downloaded"
    fi
}

install_piper() {
    step "Installing Piper TTS"
    
    local arch
    arch=$(get_arch)
    local piper_bin="/usr/local/bin/piper"
    
    if [[ -x "$piper_bin" ]] || command_exists piper; then
        success "Piper already installed"
    else
        info "Downloading Piper for $arch..."
        
        local piper_archive="piper_macos_${arch}.tar.gz"
        local piper_url="https://github.com/rhasspy/piper/releases/latest/download/${piper_archive}"
        
        # Download and extract
        curl -L --progress-bar -o "/tmp/${piper_archive}" "$piper_url" || {
            # Fallback: try without architecture suffix
            piper_archive="piper_macos.tar.gz"
            piper_url="https://github.com/rhasspy/piper/releases/latest/download/${piper_archive}"
            curl -L --progress-bar -o "/tmp/${piper_archive}" "$piper_url"
        }
        
        tar -xzf "/tmp/${piper_archive}" -C /tmp
        
        # Install (may need sudo)
        if [[ -w /usr/local/bin ]]; then
            cp /tmp/piper/piper /usr/local/bin/
        else
            sudo cp /tmp/piper/piper /usr/local/bin/
        fi
        chmod +x /usr/local/bin/piper
        
        rm -rf /tmp/piper /tmp/"${piper_archive}"
        success "Piper installed"
    fi
    
    # Download voice model
    mkdir -p "$PIPER_DIR"
    
    local voice_onnx="${PIPER_VOICE}.onnx"
    local voice_json="${PIPER_VOICE}.onnx.json"
    
    if [[ -f "$PIPER_DIR/$voice_onnx" ]]; then
        success "Piper voice already exists"
    else
        info "Downloading Piper voice: $PIPER_VOICE..."
        curl -L --progress-bar -o "$PIPER_DIR/$voice_onnx" "${PIPER_VOICE_URL}/${voice_onnx}"
        curl -L --progress-bar -o "$PIPER_DIR/$voice_json" "${PIPER_VOICE_URL}/${voice_json}"
        success "Piper voice downloaded"
    fi
}

# =============================================================================
# Configuration
# =============================================================================

configure_toml() {
    step "Configuring PiSovereign"
    
    mkdir -p "$PISOVEREIGN_CONFIG_DIR"
    
    echo -e "\n${CYAN}=== PiSovereign Configuration ===${NC}\n"
    echo "Configure your PiSovereign instance. Press Enter to skip optional settings."
    echo
    
    # Server configuration
    echo -e "${PURPLE}--- Server Settings ---${NC}"
    prompt SERVER_PORT "Server port" "3000"
    
    # Speech configuration
    echo -e "\n${PURPLE}--- Speech Processing ---${NC}"
    echo "Speech provider options:"
    echo "  1) local  - Use whisper.cpp + Piper (fully offline)"
    echo "  2) openai - Use OpenAI Whisper + TTS API"
    echo "  3) hybrid - Local STT, cloud TTS fallback"
    prompt SPEECH_PROVIDER "Speech provider (local/openai/hybrid)" "local"
    
    if [[ "$SPEECH_PROVIDER" == "openai" || "$SPEECH_PROVIDER" == "hybrid" ]]; then
        prompt_secret OPENAI_API_KEY "OpenAI API Key"
    fi
    
    # WhatsApp configuration (optional)
    echo -e "\n${PURPLE}--- WhatsApp Integration (optional) ---${NC}"
    if prompt_yes_no "Configure WhatsApp integration?"; then
        prompt_secret WA_ACCESS_TOKEN "WhatsApp Access Token"
        prompt WA_PHONE_NUMBER_ID "WhatsApp Phone Number ID" ""
        prompt_secret WA_APP_SECRET "WhatsApp App Secret"
        prompt WA_VERIFY_TOKEN "WhatsApp Verify Token" "pisovereign-verify-$(openssl rand -hex 8)"
    fi
    
    # Weather configuration
    echo -e "\n${PURPLE}--- Weather Integration ---${NC}"
    prompt WEATHER_LAT "Default latitude (e.g., 52.52 for Berlin)" "52.52"
    prompt WEATHER_LON "Default longitude (e.g., 13.405 for Berlin)" "13.405"
    
    # CalDAV configuration (optional)
    echo -e "\n${PURPLE}--- CalDAV Calendar (optional) ---${NC}"
    if prompt_yes_no "Configure CalDAV integration?"; then
        prompt CALDAV_URL "CalDAV server URL" ""
        prompt CALDAV_USER "CalDAV username" ""
        prompt_secret CALDAV_PASS "CalDAV password"
    fi
    
    # Proton Mail configuration (optional)
    echo -e "\n${PURPLE}--- Proton Mail (optional) ---${NC}"
    if prompt_yes_no "Configure Proton Mail integration?"; then
        prompt PROTON_EMAIL "Proton email address" ""
        prompt_secret PROTON_PASS "Proton Bridge password"
        prompt PROTON_IMAP_HOST "IMAP host" "127.0.0.1"
        prompt PROTON_IMAP_PORT "IMAP port" "1143"
        prompt PROTON_SMTP_HOST "SMTP host" "127.0.0.1"
        prompt PROTON_SMTP_PORT "SMTP port" "1025"
    fi
    
    # Generate API key
    echo -e "\n${PURPLE}--- API Security ---${NC}"
    local api_key
    api_key="psk_$(openssl rand -hex 32)"
    info "Generated API key: $api_key"
    warn "Save this key securely - it won't be shown again!"
    
    # Write config.toml
    write_config_toml
    
    success "Configuration saved to $PISOVEREIGN_CONFIG_DIR/config.toml"
}

write_config_toml() {
    local whisper_cmd
    whisper_cmd=$(command -v whisper-cli || echo "/opt/homebrew/bin/whisper-cli")
    
    local piper_cmd
    piper_cmd=$(command -v piper || echo "/usr/local/bin/piper")
    
    cat > "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF
# PiSovereign Configuration (macOS Development)
# Generated: $(date -Iseconds)
# =============================================================================

environment = "development"

[server]
host = "127.0.0.1"
port = ${SERVER_PORT:-3000}
cors_enabled = true
allowed_origins = []
shutdown_timeout_secs = 30
log_format = "text"

[inference]
base_url = "http://localhost:11434"
default_model = "$OLLAMA_MODEL"
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

EOF

    # WhatsApp section
    if [[ -n "${WA_ACCESS_TOKEN:-}" ]]; then
        cat >> "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF
[whatsapp]
access_token = "${WA_ACCESS_TOKEN}"
phone_number_id = "${WA_PHONE_NUMBER_ID}"
app_secret = "${WA_APP_SECRET}"
verify_token = "${WA_VERIFY_TOKEN}"
signature_required = true
api_version = "v18.0"

EOF
    fi

    # Speech section
    cat >> "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF
[speech]
provider = "${SPEECH_PROVIDER:-local}"
EOF

    if [[ -n "${OPENAI_API_KEY:-}" ]]; then
        cat >> "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF
openai_api_key = "${OPENAI_API_KEY}"
stt_model = "whisper-1"
tts_model = "tts-1"
default_voice = "nova"
EOF
    fi

    cat >> "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF

[speech.local_stt]
executable_path = "$whisper_cmd"
model_path = "$WHISPER_MODEL_DIR/$WHISPER_MODEL"
threads = 4
default_language = "de"

[speech.local_tts]
executable_path = "$piper_cmd"
default_model_path = "$PIPER_DIR/${PIPER_VOICE}.onnx"
default_voice = "$PIPER_VOICE"
output_format = "wav"
length_scale = 1.0

EOF

    # Database section
    cat >> "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF
[database]
path = "$PISOVEREIGN_DIR/data/pisovereign.db"
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

    # Weather section
    cat >> "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF
[weather]
base_url = "https://api.open-meteo.com/v1"
timeout_secs = 30
forecast_days = 7
cache_ttl_minutes = 30
default_location = { latitude = ${WEATHER_LAT:-52.52}, longitude = ${WEATHER_LON:-13.405} }

EOF

    # CalDAV section
    if [[ -n "${CALDAV_URL:-}" ]]; then
        cat >> "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF
[caldav]
server_url = "${CALDAV_URL}"
username = "${CALDAV_USER}"
password = "${CALDAV_PASS}"
verify_certs = true
timeout_secs = 30

EOF
    fi

    # Proton section
    if [[ -n "${PROTON_EMAIL:-}" ]]; then
        cat >> "$PISOVEREIGN_CONFIG_DIR/config.toml" << EOF
[proton]
imap_host = "${PROTON_IMAP_HOST:-127.0.0.1}"
imap_port = ${PROTON_IMAP_PORT:-1143}
smtp_host = "${PROTON_SMTP_HOST:-127.0.0.1}"
smtp_port = ${PROTON_SMTP_PORT:-1025}
email = "${PROTON_EMAIL}"
password = "${PROTON_PASS}"

EOF
    fi

    chmod 600 "$PISOVEREIGN_CONFIG_DIR/config.toml"
}

# =============================================================================
# Docker Compose Setup
# =============================================================================

setup_docker_compose() {
    step "Setting up Docker Compose"
    
    mkdir -p "$PISOVEREIGN_DIR"/{data,logs}
    
    # Create docker-compose.yml for development
    cat > "$PISOVEREIGN_DIR/docker-compose.yml" << EOF
# PiSovereign Docker Compose (macOS Development)
# Generated: $(date -Iseconds)

services:
  pisovereign:
    image: ghcr.io/twohreichel/pisovereign:${PISOVEREIGN_VERSION}
    container_name: pisovereign
    restart: unless-stopped
    ports:
      - "127.0.0.1:3000:3000"
      - "127.0.0.1:8080:8080"
    volumes:
      - ${PISOVEREIGN_CONFIG_DIR}/config.toml:/etc/pisovereign/config.toml:ro
      - ./data:/var/lib/pisovereign
      - ./logs:/var/log/pisovereign
    environment:
      - PISOVEREIGN_CONFIG=/etc/pisovereign/config.toml
      - PISOVEREIGN_DATA_DIR=/var/lib/pisovereign
      - PISOVEREIGN_ENVIRONMENT=development
      - RUST_LOG=debug,tower_http=debug
    extra_hosts:
      - "host.docker.internal:host-gateway"
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 10s

networks:
  default:
    name: pisovereign-net
EOF

    success "Docker Compose configuration created at $PISOVEREIGN_DIR/docker-compose.yml"
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
    
    # Wait for service to be healthy
    info "Waiting for services to start..."
    sleep 5
    
    local max_attempts=30
    local attempt=0
    while ! curl -sf http://localhost:3000/health &>/dev/null && [[ $attempt -lt $max_attempts ]]; do
        sleep 2
        ((attempt++))
    done
    
    if curl -sf http://localhost:3000/health &>/dev/null; then
        success "PiSovereign is running!"
    else
        warn "PiSovereign may still be starting. Check logs with: docker compose logs -f"
    fi
}

# =============================================================================
# Auto-Update Setup (launchd)
# =============================================================================

setup_auto_update() {
    step "Setting up automatic updates"
    
    mkdir -p "$PISOVEREIGN_DIR/scripts"
    mkdir -p "$LAUNCH_AGENTS_DIR"
    
    # Create update script
    cat > "$PISOVEREIGN_DIR/scripts/auto-update.sh" << 'SCRIPT'
#!/usr/bin/env bash
# PiSovereign Auto-Update Script (macOS)
# Runs daily via launchd

set -euo pipefail

LOG_FILE="${HOME}/.pisovereign/logs/auto-update.log"
PISOVEREIGN_DIR="${HOME}/.pisovereign"

mkdir -p "$(dirname "$LOG_FILE")"

log() {
    echo "[$(date -Iseconds)] $*" | tee -a "$LOG_FILE"
}

log "=== Starting auto-update ==="

# Update Homebrew packages
log "Updating Homebrew packages..."
brew update && brew upgrade

# Update Ollama model
log "Checking LLM model updates..."
ollama pull "$OLLAMA_MODEL" 2>&1 | tail -1

# Update Docker images
log "Updating Docker images..."
cd "$PISOVEREIGN_DIR"
docker compose pull -q

# Restart services if images changed
if docker compose up -d --no-deps 2>&1 | grep -q "Recreating"; then
    log "Services updated and restarted"
else
    log "No service updates needed"
fi

# Cleanup old Docker images
log "Cleaning up old Docker images..."
docker image prune -f

# Health check
if curl -sf http://localhost:3000/health &>/dev/null; then
    log "Health check: OK"
else
    log "Health check: FAILED - manual intervention may be required"
fi

log "=== Auto-update complete ==="
SCRIPT

    chmod +x "$PISOVEREIGN_DIR/scripts/auto-update.sh"
    
    # Create launchd plist
    cat > "$LAUNCH_AGENTS_DIR/io.pisovereign.autoupdate.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>io.pisovereign.autoupdate</string>
    <key>ProgramArguments</key>
    <array>
        <string>${PISOVEREIGN_DIR}/scripts/auto-update.sh</string>
    </array>
    <key>StartCalendarInterval</key>
    <dict>
        <key>Hour</key>
        <integer>3</integer>
        <key>Minute</key>
        <integer>0</integer>
    </dict>
    <key>StandardOutPath</key>
    <string>${PISOVEREIGN_DIR}/logs/launchd-stdout.log</string>
    <key>StandardErrorPath</key>
    <string>${PISOVEREIGN_DIR}/logs/launchd-stderr.log</string>
    <key>RunAtLoad</key>
    <false/>
</dict>
</plist>
EOF

    # Load the launchd agent
    launchctl unload "$LAUNCH_AGENTS_DIR/io.pisovereign.autoupdate.plist" 2>/dev/null || true
    launchctl load "$LAUNCH_AGENTS_DIR/io.pisovereign.autoupdate.plist"
    
    success "Auto-update configured (runs daily at 3:00 AM)"
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
        success "Ollama: OK ($(ollama list | wc -l | tr -d ' ') models)"
    else
        error "Ollama: FAILED"
        ((errors++))
    fi
    
    # Check whisper.cpp
    if command_exists whisper-cli; then
        success "whisper.cpp: OK"
    else
        error "whisper.cpp: FAILED"
        ((errors++))
    fi
    
    # Check Piper
    if command_exists piper || [[ -x /usr/local/bin/piper ]]; then
        success "Piper: OK"
    else
        error "Piper: FAILED"
        ((errors++))
    fi
    
    # Check Whisper model
    if [[ -f "$WHISPER_MODEL_DIR/$WHISPER_MODEL" ]]; then
        success "Whisper model: OK"
    else
        error "Whisper model: MISSING"
        ((errors++))
    fi
    
    # Check Piper voice
    if [[ -f "$PIPER_DIR/${PIPER_VOICE}.onnx" ]]; then
        success "Piper voice: OK"
    else
        error "Piper voice: MISSING"
        ((errors++))
    fi
    
    # Check PiSovereign
    if curl -sf http://localhost:3000/health &>/dev/null; then
        success "PiSovereign API: OK"
    else
        warn "PiSovereign API: Starting... (check docker compose logs)"
    fi
    
    # Check auto-update
    if launchctl list | grep -q "io.pisovereign.autoupdate"; then
        success "Auto-update agent: OK"
    else
        warn "Auto-update agent: Not loaded"
    fi
    
    return $errors
}

print_summary() {
    echo
    echo -e "${GREEN}============================================${NC}"
    echo -e "${GREEN}   PiSovereign Installation Complete!${NC}"
    echo -e "${GREEN}============================================${NC}"
    echo
    echo -e "${CYAN}Installation Directory:${NC} $PISOVEREIGN_DIR"
    echo -e "${CYAN}Configuration:${NC} $PISOVEREIGN_CONFIG_DIR/config.toml"
    echo -e "${CYAN}Logs:${NC} $PISOVEREIGN_DIR/logs/"
    echo
    echo -e "${CYAN}Services:${NC}"
    echo "  - PiSovereign API: http://localhost:3000"
    echo "  - Metrics: http://localhost:8080"
    echo "  - Ollama: http://localhost:11434"
    echo
    echo -e "${CYAN}Useful Commands:${NC}"
    echo "  cd $PISOVEREIGN_DIR && docker compose logs -f    # View logs"
    echo "  cd $PISOVEREIGN_DIR && docker compose restart    # Restart services"
    echo "  launchctl list | grep pisovereign                # Check auto-update"
    echo "  ollama list                                       # List LLM models"
    echo "  brew services list                                # Check Homebrew services"
    echo
    echo -e "${CYAN}Test Speech Processing:${NC}"
    echo "  whisper-cli -m \"$WHISPER_MODEL_DIR/$WHISPER_MODEL\" -f audio.wav"
    echo "  echo 'Hallo Welt' | piper --model \"$PIPER_DIR/${PIPER_VOICE}.onnx\" --output_file test.wav"
    echo
    echo -e "${CYAN}Documentation:${NC} https://twohreichel.github.io/PiSovereign/"
    echo
}

# =============================================================================
# Main
# =============================================================================

main() {
    echo -e "${PURPLE}"
    echo "╔═══════════════════════════════════════════════════════════════╗"
    echo "║                                                               ║"
    echo "║   ██████╗ ██╗███████╗ ██████╗ ██╗   ██╗███████╗██████╗       ║"
    echo "║   ██╔══██╗██║██╔════╝██╔═══██╗██║   ██║██╔════╝██╔══██╗      ║"
    echo "║   ██████╔╝██║███████╗██║   ██║██║   ██║█████╗  ██████╔╝      ║"
    echo "║   ██╔═══╝ ██║╚════██║██║   ██║╚██╗ ██╔╝██╔══╝  ██╔══██╗      ║"
    echo "║   ██║     ██║███████║╚██████╔╝ ╚████╔╝ ███████╗██║  ██║      ║"
    echo "║   ╚═╝     ╚═╝╚══════╝ ╚═════╝   ╚═══╝  ╚══════╝╚═╝  ╚═╝      ║"
    echo "║                                                               ║"
    echo "║               macOS Setup Script v1.0                         ║"
    echo "║                                                               ║"
    echo "╚═══════════════════════════════════════════════════════════════╝"
    echo -e "${NC}"
    
    # Pre-flight checks
    check_macos
    
    echo
    info "This script will install and configure PiSovereign on your Mac."
    info "The installation will take approximately 10-20 minutes."
    echo
    
    if ! prompt_yes_no "Continue with installation?" "y"; then
        echo "Installation cancelled."
        exit 0
    fi
    
    # Installation steps
    install_homebrew
    install_dependencies
    check_docker
    install_ollama
    install_whisper_cpp
    install_piper
    
    # Configuration
    configure_toml
    setup_docker_compose
    start_services
    setup_auto_update
    
    # Verification
    verify_installation
    print_summary
}

# Run main function
main "$@"
