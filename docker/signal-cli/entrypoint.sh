#!/bin/sh
set -e

SOCKET_PATH="/var/run/signal-cli/socket"

# Remove stale socket from previous runs
if [ -S "$SOCKET_PATH" ] || [ -e "$SOCKET_PATH" ]; then
  echo "Removing stale socket: $SOCKET_PATH"
  rm -f "$SOCKET_PATH"
fi

exec signal-cli --config /var/lib/signal-cli daemon --socket "$SOCKET_PATH"
