# Signal Messenger Setup

> 📱 Connect Signal messenger to PiSovereign via Docker

PiSovereign uses [signal-cli](https://github.com/AsamK/signal-cli) as a Docker container to send and receive Signal messages. This guide covers the complete setup process.

## Prerequisites

- **Docker** must be running (`docker compose up -d` in the `docker/` directory)
- **Signal app** installed on your smartphone and registered with a phone number
- **qrencode** installed on the host (for QR code display)
- **Phone number** stored in `.env` or Vault

### Installing qrencode

**macOS:**
```bash
brew install qrencode
```

**Debian / Raspberry Pi:**
```bash
sudo apt-get install qrencode
```

## Linking Your Signal Account

signal-cli is connected as a **linked device** to your existing Signal account (similar to Signal Desktop). No new account is created.

> ⚠️ **Important:** The `link` command outputs a `sgnl://` URI that must be converted into a QR code. You **cannot** pipe the output directly to `qrencode`, because `qrencode` waits for EOF — by that time the link process has already terminated and the URI has expired. Therefore, **two separate terminal commands** must be used.

### Step 1: Start the Link Process and Capture the URI

Open a terminal and run:

```bash
docker exec -it pisovereign-signal-cli signal-cli --config /var/lib/signal-cli link -n "PiSovereign" | tee /tmp/signal-uri.txt
```

This command:
1. Starts the link process **in the background**
2. Captures the URI to `/tmp/signal-uri.txt`
3. Displays the URI after 8 seconds (for verification)

### Step 2: Display the QR Code and Scan

Once the URI is displayed, generate the QR code:

```bash
head -1 /tmp/signal-uri.txt | tr -d '\n' | qrencode -t ANSIUTF8
```

Now **quickly** scan with your phone:

1. Open **Signal** on your smartphone
2. Go to **Settings → Linked Devices → Link New Device**
3. Scan the QR code shown in the terminal
4. Confirm the link on your phone

> 💡 The link process is still running in the background, waiting for the scan. If the QR code has expired, simply repeat both steps.

### Step 3: Verify the Link

After a successful scan, restart the container:

```bash
cd docker/
docker compose restart signal-cli
```

The logs should no longer show a `NotRegisteredException`:

```bash
docker compose logs signal-cli
```

## Configuration

### Phone Number

The Signal phone number must be known to PiSovereign. Use one of the following methods:

**Option A: `.env` file** (in the `docker/` directory):
```bash
PISOVEREIGN_SIGNAL__PHONE_NUMBER=+491234567890
```

**Option B: Vault:**
```bash
vault kv put secret/pisovereign signal_phone_number="+491234567890"
```

### config.toml

```toml
messenger = "signal"

[signal]
phone_number = "+491234567890"        # E.164 format
socket_path = "/var/run/signal-cli/socket"
timeout_ms = 30000
```

### Environment Variables

```bash
export PISOVEREIGN_MESSENGER=signal
export PISOVEREIGN_SIGNAL__PHONE_NUMBER=+491234567890
export PISOVEREIGN_SIGNAL__SOCKET_PATH=/var/run/signal-cli/socket
```

## Troubleshooting

### Socket Already in Use

```
Failed to bind socket /var/run/signal-cli/socket: Address already in use
```

**Cause:** A stale socket from a previous run persists in the Docker volume.

**Solution:** The container uses an entrypoint script that automatically cleans up the socket before starting. If the error still occurs:

```bash
docker compose restart signal-cli
```

### NotRegisteredException

```
WARN MultiAccountManager - Ignoring +49...: User is not registered.
```

**Cause:** signal-cli has not been linked to a Signal account.

**Solution:** Complete the [account linking](#linking-your-signal-account) procedure.

### Expired QR Code

**Cause:** `qrencode` waits for EOF. When piping `signal-cli link | qrencode`, the QR code is only displayed after the link process terminates — at which point the URI is already invalid.

**Solution:** Redirect the URI to a file (Step 1) and display it as a QR code separately (Step 2). See [Linking Your Signal Account](#linking-your-signal-account).

### Daemon Connection Failed

```bash
# Check the socket
docker exec pisovereign-signal-cli ls -la /var/run/signal-cli/socket

# Check container logs
docker compose logs signal-cli
```

## Security

- Signal messages are end-to-end encrypted
- signal-cli stores cryptographic keys locally in the `signal-cli-data` volume
- The socket (`signal-cli-socket`) is shared only within the Docker network

### Backup

The signal-cli data should be backed up regularly:

```bash
docker run --rm -v docker_signal-cli-data:/data -v $(pwd):/backup \
  alpine tar czf /backup/signal-cli-backup.tar.gz -C /data .
```

See [Backup & Restore](../operations/backup-restore.md) for complete backup procedures.

## See Also

- [Docker Setup](./docker-setup.md) — Set up the Docker environment
- [Vault Setup](./vault-setup.md) — Manage secrets
- [Configuration Reference](./configuration.md) — All configuration options
- [signal-cli Documentation](https://github.com/AsamK/signal-cli) — Upstream documentation
