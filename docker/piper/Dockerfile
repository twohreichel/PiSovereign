# Piper TTS Docker container for PiSovereign
# Downloads Piper binary and German voice model
# Exposes HTTP API via lightweight Python wrapper
# Compatible with aarch64 (Raspberry Pi 5) and amd64

FROM debian:bookworm-slim

ARG PIPER_VERSION=2023.11.14-2
ARG TARGETARCH

LABEL org.opencontainers.image.title="PiSovereign Piper TTS"
LABEL org.opencontainers.image.description="Piper text-to-speech server for PiSovereign"

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      curl \
      ca-certificates \
      python3 && \
    rm -rf /var/lib/apt/lists/*

# Map Docker TARGETARCH to Piper naming convention
RUN PIPER_ARCH="" && \
    case "${TARGETARCH}" in \
      arm64) PIPER_ARCH="aarch64" ;; \
      amd64) PIPER_ARCH="x86_64" ;; \
      *) echo "Unsupported architecture: ${TARGETARCH}" && exit 1 ;; \
    esac && \
    mkdir -p /opt/piper && \
    curl -fsSL "https://github.com/rhasspy/piper/releases/download/${PIPER_VERSION}/piper_linux_${PIPER_ARCH}.tar.gz" \
      | tar xz -C /opt && \
    chmod +x /opt/piper/piper && \
    ln -s /opt/piper/piper /usr/local/bin/piper

# Download German voice model (Thorsten medium)
RUN mkdir -p /models && \
    curl -fsSL -o /models/de_DE-thorsten-medium.onnx \
      "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/de/de_DE/thorsten/medium/de_DE-thorsten-medium.onnx" && \
    curl -fsSL -o /models/de_DE-thorsten-medium.onnx.json \
      "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/de/de_DE/thorsten/medium/de_DE-thorsten-medium.onnx.json"

# Create non-root user
RUN groupadd -r piper && \
    useradd -r -g piper -s /sbin/nologin -d /app piper && \
    chown -R piper:piper /models

# Copy HTTP wrapper
COPY piper-http-server.py /app/piper-http-server.py

VOLUME ["/models"]

USER piper

EXPOSE 8082

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD curl -sf http://localhost:8082/health || exit 1

ENTRYPOINT ["python3", "/app/piper-http-server.py"]
