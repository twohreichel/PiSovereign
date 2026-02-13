#!/bin/sh
# Vault initialization and bootstrap script for PiSovereign
set -e

VAULT_ADDR="${VAULT_ADDR:-http://vault:8200}"
INIT_OUTPUT="/vault/init-output/vault-keys.json"
export VAULT_ADDR

log() {
  echo "[vault-init] $(date '+%Y-%m-%d %H:%M:%S') $1"
}

# Helper: extract a JSON string value by key (handles multiline JSON)
json_val() {
  tr -d '\n\r' | sed -n 's/.*"'"$1"'" *: *"\([^"]*\)".*/\1/p' | head -1
}

# Helper: extract first element from a JSON array of strings
json_arr_first() {
  tr -d '\n\r' | tr -s ' ' | sed -n 's/.*"'"$1"'" *: *\[ *"\([^"]*\)".*/\1/p' | head -1
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
      UNSEAL_KEY=$(cat "${INIT_OUTPUT}" | json_val unseal_key)
      if [ -z "${UNSEAL_KEY}" ]; then
        log "ERROR: Could not parse unseal key from ${INIT_OUTPUT}"
        cat "${INIT_OUTPUT}"
        exit 1
      fi
      vault operator unseal -address="${VAULT_ADDR}" "${UNSEAL_KEY}" > /dev/null
      log "Vault unsealed successfully."
    else
      log "ERROR: Vault is sealed but no keys found at ${INIT_OUTPUT}"
      exit 1
    fi
  else
    log "Vault is already unsealed."
  fi

  # Login with root token
  if [ -f "${INIT_OUTPUT}" ]; then
    ROOT_TOKEN=$(cat "${INIT_OUTPUT}" | json_val root_token)
    export VAULT_TOKEN="${ROOT_TOKEN}"
  elif [ -n "${VAULT_TOKEN}" ]; then
    log "Using VAULT_TOKEN from environment."
  else
    log "ERROR: No root token available."
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

UNSEAL_KEY=$(echo "${INIT_RESULT}" | json_arr_first unseal_keys_b64)
ROOT_TOKEN=$(echo "${INIT_RESULT}" | json_val root_token)

if [ -z "${UNSEAL_KEY}" ] || [ -z "${ROOT_TOKEN}" ]; then
  log "ERROR: Failed to parse init result:"
  echo "${INIT_RESULT}"
  exit 1
fi

# Save keys to persistent volume
mkdir -p "$(dirname "${INIT_OUTPUT}")"
printf '{\n  "unseal_key": "%s",\n  "root_token": "%s",\n  "initialized_at": "%s",\n  "warning": "KEEP THIS FILE SECURE"\n}\n' \
  "${UNSEAL_KEY}" "${ROOT_TOKEN}" "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" > "${INIT_OUTPUT}"
chmod 600 "${INIT_OUTPUT}"
log "Vault keys saved to ${INIT_OUTPUT}"
log "Root Token: ${ROOT_TOKEN}"

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
vault kv put -address="${VAULT_ADDR}" secret/pisovereign/whatsapp access_token="" app_secret="" verify_token="" 2>/dev/null || true
vault kv put -address="${VAULT_ADDR}" secret/pisovereign/signal phone_number="" 2>/dev/null || true
vault kv put -address="${VAULT_ADDR}" secret/pisovereign/proton email="" password="" 2>/dev/null || true
vault kv put -address="${VAULT_ADDR}" secret/pisovereign/caldav username="" password="" 2>/dev/null || true
vault kv put -address="${VAULT_ADDR}" secret/pisovereign/websearch brave_api_key="" 2>/dev/null || true
vault kv put -address="${VAULT_ADDR}" secret/pisovereign/security api_keys="[]" 2>/dev/null || true
vault kv put -address="${VAULT_ADDR}" secret/pisovereign/speech openai_api_key="" 2>/dev/null || true

log "Vault bootstrap complete!"
