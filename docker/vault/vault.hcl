# HashiCorp Vault server configuration for PiSovereign
# Storage: file-based (no external dependencies)
# Listener: plaintext TCP (TLS terminated by Traefik externally)

storage "file" {
  path = "/vault/data"
}

listener "tcp" {
  address     = "0.0.0.0:8200"
  tls_disable = true
}

# Required for Docker environments
disable_mlock = true

# API address for internal communication
api_addr = "http://vault:8200"

# Disable the web UI in production
ui = false

# Telemetry (optional, for Prometheus scraping)
telemetry {
  disable_hostname = true
}
