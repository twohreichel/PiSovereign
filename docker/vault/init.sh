#!/bin/sh
# Vault initialization and bootstrap script for PiSovereign
# This script runs as an init container to:
# 1. Wait for Vault to be ready
# 2. Initialize Vault (first run only)
# 3. Unseal Vault
# 4. Enable KV v2 secrets engine
# 5. Create PiSovereign policy
# 6. Seed initial empty secret structure

set -e

VAULT_ADDR="${VAULT_ADDR:-http://vault:8200}"
INIT_OUTPUT="/vault/init-output/vault-keys.json"
export VAULT_ADDR

log() {
  echo "[vault-init] $(date '+%Y-%m-%d %H:%M:%S') $1"
}

# Wait for Vault to be reachable
log "Waiting for Vault at ${VAULT_ADDR}..."
until vault status -address="${VAULT_ADDR}" 2>/dev/null | grep -q "Seal Type"; do
  sleep 2
done
log "Vault is reachable."

# Check if already initialized
if vault status -address="${VAULT_ADDR}" 2>/dev/null | grep -q "Initialized.*true"; then
  log "Vault is already initialized."

  # Check if sealed
  if vault status -address="${VAULT_ADDR}" 2>/dev/null | grep -q "Sealed.*true"; then
    log "Vault is sealed. Attempting unseal..."
    if [ -f "${INIT_OUTPUT}" ]; then
      UNSEAL_KEY=$(cat "${INIT_OUTPUT}" | grep -o '"unseal_key":"[^"]*"' | cut -d'"' -f4)
      vault operator unseal -address="${VAULT_ADDR}" "${UNSEAL_KEY}" > /dev/null
      log "Vault unsealed successfully."
    else
      log "ERROR: Vault is sealed but no keys found at ${INIT_OUTPUT}"
      log "Manual unseal required: vault operator unseal <key>"
      exit 1
    fi
  else
    log "Vault is already unsealed."
  fi

  # Login with root token
  if [ -f "${INIT_OUTPUT}" ]; then
    ROOT_TOKEN=$(cat "${INIT_OUTPUT}" | grep -o '"root_token":"[^"]*"' | cut -d'"' -f4)
    export VAULT_TOKEN="${ROOT_TOKEN}"
  elif [ -n "${VAULT_TOKEN}" ]; then
    log "Using VAULT_TOKEN from environment."
  else
    log "ERROR: No root token available. Set VAULT_TOKEN environment variable."
    exit 1
  fi

  log "Vault initialization check complete."
  exit 0
fi

# --- First-time initialization ---
log "Initializing Vault for the first time..."

INIT_RESULT=$(vault operator init \
  -address="${VAULT_ADDR}" \
  -key-shares=1 \
  -key-threshold=1 \
  -format=json)

UNSEAL_KEY=$(echo "${INIT_RESULT}" | grep -o '"unseal_keys_b64":\[\"[^"]*' | cut -d'"' -f4)
ROOT_TOKEN=$(echo "${INIT_RESULT}" | grep -o '"root_token":"[^"]*"' | cut -d'"' -f4)

# Save keys to persistent volume
mkdir -p "$(dirname "${INIT_OUTPUT}")"
cat > "${INIT_OUTPUT}" << EOF
{
  "unseal_key": "${UNSEAL_KEY}",
  "root_token": "${ROOT_TOKEN}",
  "initialized_at": "$(date -u '+%Y-%m-%dT%H:%M:%SZ')",
  "warning": "KEEP THIS FILE SECURE. It contains the Vault unseal key and root token."
}
EOF
chmod 600 "${INIT_OUTPUT}"
log "Vault keys saved to ${INIT_OUTPUT}"
log "============================================"
log "  IMPORTANT: Back up ${INIT_OUTPUT}"
log "  Root Token: ${ROOT_TOKEN}"
log "============================================"

# Unseal
log "Unsealing Vault..."
vault operator unseal -address="${VAULT_ADDR}" "${UNSEAL_KEY}" > /dev/null
log "Vault unsealed."

# Authenticate
export VAULT_TOKEN="${ROOT_TOKEN}"

# Enable KV v2 secrets engine
log "Enabling KV v2 secrets engine at secret/..."
vault secrets enable -address="${VAULT_ADDR}" -path=secret kv-v2 2>/dev/null || \
  log "KV v2 engine already enabled at secret/"

# Create PiSovereign policy
log "Creating PiSovereign policy..."
vault policy write -address="${VAULT_ADDR}" pisovereign-policy - << 'POLICY'
# PiSovereign application policy
# Read-only access to application secrets
path "secret/data/pisovereign/*" {
  capabilities = ["read", "list"]
}

path "secret/metadata/pisovereign/*" {
  capabilities = ["read", "list"]
}
POLICY
log "Policy 'pisovereign-policy' created."

# Seed initial empty secret structure
log "Seeding initial secret structure..."

vault kv put -address="${VAULT_ADDR}" secret/pisovereign/whatsapp \
  access_token="" \
  app_secret="" \
  verify_token="" 2>/dev/null || true

vault kv put -address="${VAULT_ADDR}" secret/pisovereign/signal \
  phone_number="" 2>/dev/null || true

vault kv put -address="${VAULT_ADDR}" secret/pisovereign/proton \
  email="" \
  password="" 2>/dev/null || true

vault kv put -address="${VAULT_ADDR}" secret/pisovereign/caldav \
  username="" \
  password="" 2>/dev/null || true

vault kv put -address="${VAULT_ADDR}" secret/pisovereign/websearch \
  brave_api_key="" 2>/dev/null || true

vault kv put -address="${VAULT_ADDR}" secret/pisovereign/security \
  api_keys="[]" 2>/dev/null || true

vault kv put -address="${VAULT_ADDR}" secret/pisovereign/speech \
  openai_api_key="" 2>/dev/null || true

log "Initial secret structure created."
log "============================================"
log "  Vault bootstrap complete!"
log "  Add your secrets with:"
log "  docker compose exec vault vault kv put secret/pisovereign/<service> key=value"
log "============================================"
