# whisper.cpp Docker container for PiSovereign
# Builds whisper.cpp from source with HTTP server
# Compatible with aarch64 (Raspberry Pi 5) and amd64

# --- Build stage ---
FROM debian:bookworm-slim AS builder

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      build-essential \
      cmake \
      git \
      curl \
      ca-certificates && \
    rm -rf /var/lib/apt/lists/*

ARG WHISPER_CPP_VERSION=v1.7.4

RUN git clone --depth 1 --branch "${WHISPER_CPP_VERSION}" \
      https://github.com/ggerganov/whisper.cpp.git /build/whisper.cpp

WORKDIR /build/whisper.cpp

RUN cmake -B build \
      -DCMAKE_BUILD_TYPE=Release \
      -DWHISPER_BUILD_SERVER=ON \
      -DWHISPER_BUILD_EXAMPLES=OFF && \
    cmake --build build --config Release -j "$(nproc)" --target whisper-server

# Download base model
RUN curl -fsSL -o /build/ggml-base.bin \
      "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin"

# --- Runtime stage ---
FROM debian:bookworm-slim

LABEL org.opencontainers.image.title="PiSovereign Whisper STT"
LABEL org.opencontainers.image.description="whisper.cpp speech-to-text server for PiSovereign"

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      curl \
      ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r whisper && \
    useradd -r -g whisper -s /sbin/nologin -d /app whisper

# Copy binary and model
COPY --from=builder /build/whisper.cpp/build/bin/whisper-server /usr/local/bin/whisper-server
COPY --from=builder /build/ggml-base.bin /models/ggml-base.bin

RUN chown -R whisper:whisper /models

VOLUME ["/models"]

USER whisper

EXPOSE 8081

HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
  CMD curl -sf http://localhost:8081/health || exit 1

ENTRYPOINT ["whisper-server", \
  "--model", "/models/ggml-base.bin", \
  "--host", "0.0.0.0", \
  "--port", "8081", \
  "--threads", "4"]
