#!/bin/sh
# Ollama model initialization script for PiSovereign
# Waits for Ollama to be ready, then pulls required models

set -e

OLLAMA_HOST="${OLLAMA_HOST:-http://ollama:11434}"

log() {
  echo "[ollama-init] $(date '+%Y-%m-%d %H:%M:%S') $1"
}

# Wait for Ollama to be ready
log "Waiting for Ollama at ${OLLAMA_HOST}..."
until curl -sf "${OLLAMA_HOST}/api/tags" > /dev/null 2>&1; do
  sleep 3
done
log "Ollama is ready."

# Pull inference model
log "Pulling qwen2.5:1.5b (this may take several minutes on first run)..."
ollama pull qwen2.5:1.5b
log "Model qwen2.5:1.5b ready."

# Pull embedding model
log "Pulling nomic-embed-text..."
ollama pull nomic-embed-text
log "Model nomic-embed-text ready."

log "All models downloaded successfully."
