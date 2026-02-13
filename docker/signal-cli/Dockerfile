# signal-cli Docker container for PiSovereign
# Runs signal-cli in JSON-RPC daemon mode over Unix socket
# Compatible with aarch64 (Raspberry Pi 5) and amd64

FROM --platform=linux/amd64 eclipse-temurin:21-jre-jammy AS runtime

ARG SIGNAL_CLI_VERSION=0.13.4
ARG TARGETARCH

LABEL org.opencontainers.image.title="PiSovereign signal-cli"
LABEL org.opencontainers.image.description="signal-cli JSON-RPC daemon for PiSovereign"
LABEL org.opencontainers.image.version="${SIGNAL_CLI_VERSION}"

# Install minimal dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      curl \
      ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Create signal-cli user
RUN groupadd -r signal-cli && \
    useradd -r -g signal-cli -s /sbin/nologin -d /var/lib/signal-cli signal-cli

# Download and install signal-cli
RUN curl -fsSL "https://github.com/AsamK/signal-cli/releases/download/v${SIGNAL_CLI_VERSION}/signal-cli-${SIGNAL_CLI_VERSION}.tar.gz" \
      | tar xz -C /opt --strip-components=1 && \
    ln -s /opt/bin/signal-cli /usr/local/bin/signal-cli && \
    chmod +x /usr/local/bin/signal-cli

# Create runtime directories
RUN mkdir -p /var/run/signal-cli /var/lib/signal-cli && \
    chown -R signal-cli:signal-cli /var/run/signal-cli /var/lib/signal-cli

VOLUME ["/var/run/signal-cli", "/var/lib/signal-cli"]

USER signal-cli

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD test -S /var/run/signal-cli/socket || exit 1

ENTRYPOINT ["signal-cli", "--config", "/var/lib/signal-cli", "daemon", "--socket", "/var/run/signal-cli/socket"]
