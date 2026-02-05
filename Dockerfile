# =============================================================================
# PiSovereign Multi-Stage Dockerfile
# Optimized for Raspberry Pi 5 with Hailo-10H AI HAT+
# =============================================================================
#
# Build: docker build -t pisovereign .
# Run:   docker run -d -p 3000:3000 --device /dev/hailo0 pisovereign
#
# Multi-arch support: linux/amd64, linux/arm64
# =============================================================================

# -----------------------------------------------------------------------------
# Stage 1: Build Environment
# -----------------------------------------------------------------------------
FROM rust:1.93-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libssl-dev \
    libsqlite3-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create build directory
WORKDIR /build

# Copy workspace configuration first (for better layer caching)
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./

# Copy all crate manifests
COPY crates/domain/Cargo.toml crates/domain/
COPY crates/application/Cargo.toml crates/application/
COPY crates/infrastructure/Cargo.toml crates/infrastructure/
COPY crates/ai_core/Cargo.toml crates/ai_core/
COPY crates/presentation_http/Cargo.toml crates/presentation_http/
COPY crates/presentation_cli/Cargo.toml crates/presentation_cli/
COPY crates/integration_whatsapp/Cargo.toml crates/integration_whatsapp/
COPY crates/integration_caldav/Cargo.toml crates/integration_caldav/
COPY crates/integration_proton/Cargo.toml crates/integration_proton/

# Create dummy source files to cache dependency compilation
RUN mkdir -p crates/domain/src && echo "pub fn dummy() {}" > crates/domain/src/lib.rs && \
    mkdir -p crates/application/src && echo "pub fn dummy() {}" > crates/application/src/lib.rs && \
    mkdir -p crates/infrastructure/src && echo "pub fn dummy() {}" > crates/infrastructure/src/lib.rs && \
    mkdir -p crates/ai_core/src && echo "pub fn dummy() {}" > crates/ai_core/src/lib.rs && \
    mkdir -p crates/presentation_http/src && echo "fn main() {}" > crates/presentation_http/src/main.rs && \
    mkdir -p crates/presentation_cli/src && echo "fn main() {}" > crates/presentation_cli/src/main.rs && \
    mkdir -p crates/integration_whatsapp/src && echo "pub fn dummy() {}" > crates/integration_whatsapp/src/lib.rs && \
    mkdir -p crates/integration_caldav/src && echo "pub fn dummy() {}" > crates/integration_caldav/src/lib.rs && \
    mkdir -p crates/integration_proton/src && echo "pub fn dummy() {}" > crates/integration_proton/src/lib.rs

# Build dependencies only (cached layer)
RUN cargo build --release --workspace 2>/dev/null || true

# Remove dummy sources
RUN find crates -name "*.rs" -delete

# Copy actual source code
COPY crates/ crates/

# Touch source files to invalidate cache for actual build
RUN find crates -name "*.rs" -exec touch {} +

# Build release binaries with optimizations
ENV CARGO_PROFILE_RELEASE_LTO=thin
ENV CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
ENV CARGO_PROFILE_RELEASE_OPT_LEVEL=3
ENV CARGO_PROFILE_RELEASE_STRIP=symbols

RUN cargo build --release --workspace

# Verify binaries exist
RUN test -f /build/target/release/pisovereign-server && \
    test -f /build/target/release/pisovereign-cli

# -----------------------------------------------------------------------------
# Stage 2: Runtime Environment
# -----------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# Labels for OCI compliance
LABEL org.opencontainers.image.title="PiSovereign"
LABEL org.opencontainers.image.description="AI-powered personal assistant for Raspberry Pi 5 with Hailo-10H"
LABEL org.opencontainers.image.vendor="Andreas Reichel"
LABEL org.opencontainers.image.licenses="MIT"
LABEL org.opencontainers.image.source="https://github.com/andreasreichel/PiSovereign"

# Install runtime dependencies and Hailo SDK
RUN apt-get update && apt-get install -y --no-install-recommends \
    # Runtime libraries
    libssl3 \
    libsqlite3-0 \
    ca-certificates \
    # Hailo SDK dependencies
    wget \
    gnupg \
    && rm -rf /var/lib/apt/lists/*

# Install Hailo SDK 4.20 (ARM64 and AMD64 compatible)
# Note: Hailo SDK requires registration at hailo.ai for production use
ARG HAILO_SDK_VERSION=4.20.0
ENV HAILO_SDK_VERSION=${HAILO_SDK_VERSION}

# Add Hailo repository and install runtime
# This uses the public Hailo APT repository
RUN arch=$(dpkg --print-architecture) && \
    if [ "$arch" = "arm64" ] || [ "$arch" = "amd64" ]; then \
        wget -qO - https://hailo.ai/downloads/hailo-apt-key.pub | gpg --dearmor -o /usr/share/keyrings/hailo-archive-keyring.gpg && \
        echo "deb [arch=$arch signed-by=/usr/share/keyrings/hailo-archive-keyring.gpg] https://hailo.ai/downloads/apt bookworm main" > /etc/apt/sources.list.d/hailo.list && \
        apt-get update && \
        apt-get install -y --no-install-recommends \
            hailort=${HAILO_SDK_VERSION}* \
        || echo "Hailo SDK installation skipped (may require manual setup)" && \
        rm -rf /var/lib/apt/lists/*; \
    fi

# Create non-root user for security
RUN groupadd --gid 1000 pisovereign && \
    useradd --uid 1000 --gid pisovereign --shell /bin/false --create-home pisovereign

# Create application directories
RUN mkdir -p /app/data /app/config /app/logs && \
    chown -R pisovereign:pisovereign /app

# Copy binaries from builder
COPY --from=builder --chown=pisovereign:pisovereign /build/target/release/pisovereign-server /app/
COPY --from=builder --chown=pisovereign:pisovereign /build/target/release/pisovereign-cli /app/

# Copy default configuration
COPY --chown=pisovereign:pisovereign config.toml /app/config/

# Set working directory
WORKDIR /app

# Switch to non-root user
USER pisovereign

# Environment variables
ENV RUST_LOG=info
ENV PISOVEREIGN_CONFIG=/app/config/config.toml
ENV PISOVEREIGN_DATA_DIR=/app/data

# Expose ports
# HTTP API
EXPOSE 3000
# Metrics (Prometheus)
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD ["/app/pisovereign-cli", "health"] || exit 1

# Default command: run the server
ENTRYPOINT ["/app/pisovereign-server"]
CMD ["--config", "/app/config/config.toml"]
