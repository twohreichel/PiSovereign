# Backup & Restore

> 💾 Protect your PiSovereign data with comprehensive backup strategies

This guide covers backup procedures, automated backups, and disaster recovery.

## Table of Contents

- [Overview](#overview)
- [What to Back Up](#what-to-back-up)
- [Database Backup](#database-backup)
  - [Manual Backup](#manual-backup)
  - [Automated Backups](#automated-backups)
- [S3-Compatible Storage](#s3-compatible-storage)
  - [Configuration](#s3-configuration)
  - [S3 Backup Commands](#s3-backup-commands)
- [Full System Backup](#full-system-backup)
- [Restore Procedures](#restore-procedures)
  - [Database Restore](#database-restore)
  - [Configuration Restore](#configuration-restore)
  - [Disaster Recovery](#disaster-recovery)
- [Backup Verification](#backup-verification)
- [Retention Policy](#retention-policy)

---

## Overview

Backup strategy overview:

| Component | Method | Frequency | Retention |
|-----------|--------|-----------|-----------|
| **Database** | SQLite copy | Daily | 7 daily, 4 weekly, 12 monthly |
| **Configuration** | File copy | On change | 5 versions |
| **Vault Secrets** | Vault backup | Weekly | 4 weekly |
| **Full System** | SD/NVMe image | Monthly | 3 monthly |

---

## What to Back Up

### Critical Data

| Path | Contents | Priority |
|------|----------|----------|
| `/var/lib/pisovereign/pisovereign.db` | Conversations, approvals, audit logs | **High** |
| `/etc/pisovereign/config.toml` | Application configuration | **High** |
| `/opt/vault/data` | Vault storage (if local) | **High** |

### Important Data

| Path | Contents | Priority |
|------|----------|----------|
| `/var/lib/pisovereign/cache.redb` | Persistent cache | Medium |
| `/opt/hailo/models` | Downloaded models | Medium |
| `/etc/pisovereign/env` | Environment overrides | Medium |

### Can Be Recreated

| Path | Contents | Priority |
|------|----------|----------|
| Prometheus data | Metrics | Low |
| Grafana dashboards | Can reimport | Low |
| Log files | Historical only | Low |

---

## Database Backup

### Manual Backup

Using the PiSovereign CLI:

```bash
# Simple local backup
pisovereign-cli backup --output /backup/pisovereign-$(date +%Y%m%d).db

# With timestamp
pisovereign-cli backup \
  --output /backup/pisovereign-$(date +%Y%m%d_%H%M%S).db

# Compressed backup
pisovereign-cli backup --output - | gzip > /backup/pisovereign-$(date +%Y%m%d).db.gz
```

Using SQLite directly:

```bash
# Online backup (safe while running)
sqlite3 /var/lib/pisovereign/pisovereign.db ".backup /backup/pisovereign.db"

# With vacuum (smaller file)
sqlite3 /var/lib/pisovereign/pisovereign.db "VACUUM INTO '/backup/pisovereign.db'"
```

### Automated Backups

Create backup script:

```bash
sudo nano /usr/local/bin/pisovereign-backup.sh
```

```bash
#!/bin/bash
set -euo pipefail

# Configuration
BACKUP_DIR="/backup/pisovereign"
DB_PATH="/var/lib/pisovereign/pisovereign.db"
RETENTION_DAILY=7
RETENTION_WEEKLY=4
RETENTION_MONTHLY=12

# Create directories
mkdir -p "$BACKUP_DIR"/{daily,weekly,monthly}

# Timestamp
DATE=$(date +%Y%m%d)
DAY_OF_WEEK=$(date +%u)
DAY_OF_MONTH=$(date +%d)

# Daily backup
DAILY_FILE="$BACKUP_DIR/daily/pisovereign-$DATE.db.gz"
echo "Creating daily backup: $DAILY_FILE"
sqlite3 "$DB_PATH" ".backup /tmp/pisovereign-backup.db"
gzip -c /tmp/pisovereign-backup.db > "$DAILY_FILE"
rm /tmp/pisovereign-backup.db

# Weekly backup (Sunday)
if [ "$DAY_OF_WEEK" -eq 7 ]; then
    WEEKLY_FILE="$BACKUP_DIR/weekly/pisovereign-week$(date +%V)-$DATE.db.gz"
    echo "Creating weekly backup: $WEEKLY_FILE"
    cp "$DAILY_FILE" "$WEEKLY_FILE"
fi

# Monthly backup (1st of month)
if [ "$DAY_OF_MONTH" -eq "01" ]; then
    MONTHLY_FILE="$BACKUP_DIR/monthly/pisovereign-$(date +%Y%m).db.gz"
    echo "Creating monthly backup: $MONTHLY_FILE"
    cp "$DAILY_FILE" "$MONTHLY_FILE"
fi

# Cleanup old backups
echo "Cleaning up old backups..."
find "$BACKUP_DIR/daily" -name "*.db.gz" -mtime +$RETENTION_DAILY -delete
find "$BACKUP_DIR/weekly" -name "*.db.gz" -mtime +$((RETENTION_WEEKLY * 7)) -delete
find "$BACKUP_DIR/monthly" -name "*.db.gz" -mtime +$((RETENTION_MONTHLY * 30)) -delete

# Backup config
CONFIG_BACKUP="$BACKUP_DIR/config/config-$DATE.toml"
mkdir -p "$BACKUP_DIR/config"
cp /etc/pisovereign/config.toml "$CONFIG_BACKUP"
find "$BACKUP_DIR/config" -name "*.toml" -mtime +30 -delete

echo "Backup completed successfully"
```

```bash
sudo chmod +x /usr/local/bin/pisovereign-backup.sh
```

Schedule with cron:

```bash
sudo crontab -e
```

```cron
# Daily backup at 2 AM
0 2 * * * /usr/local/bin/pisovereign-backup.sh >> /var/log/pisovereign-backup.log 2>&1
```

---

## S3-Compatible Storage

### S3 Configuration

PiSovereign CLI supports S3-compatible storage (AWS S3, MinIO, Backblaze B2):

```bash
# Environment variables
export AWS_ACCESS_KEY_ID="your-access-key"
export AWS_SECRET_ACCESS_KEY="your-secret-key"
```

Or in configuration file:

```toml
# /etc/pisovereign/backup.toml
[s3]
bucket = "pisovereign-backups"
region = "eu-central-1"
endpoint = "https://s3.eu-central-1.amazonaws.com"
# For MinIO or Backblaze B2:
# endpoint = "https://s3.example.com"
```

### S3 Backup Commands

```bash
# Backup to S3
pisovereign-cli backup \
  --s3-bucket pisovereign-backups \
  --s3-region eu-central-1 \
  --s3-prefix daily/ \
  --s3-access-key "$AWS_ACCESS_KEY_ID" \
  --s3-secret-key "$AWS_SECRET_ACCESS_KEY"

# With custom endpoint (MinIO)
pisovereign-cli backup \
  --s3-bucket pisovereign-backups \
  --s3-endpoint https://minio.local:9000 \
  --s3-access-key "$MINIO_ACCESS_KEY" \
  --s3-secret-key "$MINIO_SECRET_KEY"

# List backups in S3
aws s3 ls s3://pisovereign-backups/daily/
```

Automated S3 backup script:

```bash
#!/bin/bash
set -euo pipefail

DATE=$(date +%Y%m%d)

# Upload to S3
pisovereign-cli backup \
  --s3-bucket pisovereign-backups \
  --s3-region eu-central-1 \
  --s3-prefix "daily/pisovereign-$DATE.db.gz" \
  --s3-access-key "$AWS_ACCESS_KEY_ID" \
  --s3-secret-key "$AWS_SECRET_ACCESS_KEY"

# Configure S3 lifecycle for automatic cleanup (one-time setup)
# aws s3api put-bucket-lifecycle-configuration \
#   --bucket pisovereign-backups \
#   --lifecycle-configuration file://lifecycle.json
```

S3 lifecycle policy (`lifecycle.json`):

```json
{
  "Rules": [
    {
      "ID": "DeleteOldDailyBackups",
      "Status": "Enabled",
      "Filter": { "Prefix": "daily/" },
      "Expiration": { "Days": 7 }
    },
    {
      "ID": "DeleteOldWeeklyBackups",
      "Status": "Enabled",
      "Filter": { "Prefix": "weekly/" },
      "Expiration": { "Days": 30 }
    },
    {
      "ID": "DeleteOldMonthlyBackups",
      "Status": "Enabled",
      "Filter": { "Prefix": "monthly/" },
      "Expiration": { "Days": 365 }
    }
  ]
}
```

---

## Full System Backup

### SD Card / NVMe Image

Create full system image for disaster recovery:

```bash
# Identify storage device
lsblk

# Create image (run from another system or boot USB)
sudo dd if=/dev/mmcblk0 of=/backup/pisovereign-full-$(date +%Y%m%d).img bs=4M status=progress

# Compress (takes a while)
gzip /backup/pisovereign-full-$(date +%Y%m%d).img
```

### Incremental System Backup

Using rsync for incremental backups:

```bash
#!/bin/bash
# /usr/local/bin/pisovereign-system-backup.sh

BACKUP_DIR="/backup/system"
DATE=$(date +%Y%m%d)
LATEST="$BACKUP_DIR/latest"

mkdir -p "$BACKUP_DIR/$DATE"

rsync -aHAX --delete \
  --exclude='/proc/*' \
  --exclude='/sys/*' \
  --exclude='/dev/*' \
  --exclude='/tmp/*' \
  --exclude='/run/*' \
  --exclude='/mnt/*' \
  --exclude='/media/*' \
  --exclude='/backup/*' \
  --link-dest="$LATEST" \
  / "$BACKUP_DIR/$DATE/"

rm -f "$LATEST"
ln -s "$BACKUP_DIR/$DATE" "$LATEST"
```

---

## Restore Procedures

### Database Restore

```bash
# Stop the service
sudo systemctl stop pisovereign

# Backup current database (just in case)
cp /var/lib/pisovereign/pisovereign.db /var/lib/pisovereign/pisovereign.db.pre-restore

# Restore from backup
gunzip -c /backup/pisovereign/daily/pisovereign-20260207.db.gz > /var/lib/pisovereign/pisovereign.db

# Or using CLI
pisovereign-cli restore --input /backup/pisovereign-20260207.db

# Verify integrity
sqlite3 /var/lib/pisovereign/pisovereign.db "PRAGMA integrity_check;"

# Set permissions
sudo chown pisovereign:pisovereign /var/lib/pisovereign/pisovereign.db

# Start service
sudo systemctl start pisovereign

# Verify
pisovereign-cli status
```

### Restore from S3

```bash
# Download from S3
aws s3 cp s3://pisovereign-backups/daily/pisovereign-20260207.db.gz /tmp/

# Or using CLI
pisovereign-cli restore \
  --s3-bucket pisovereign-backups \
  --s3-key daily/pisovereign-20260207.db.gz \
  --s3-region eu-central-1
```

### Configuration Restore

```bash
# Restore config
sudo cp /backup/pisovereign/config/config-20260207.toml /etc/pisovereign/config.toml

# Verify syntax
pisovereign-cli config validate

# Restart service
sudo systemctl restart pisovereign
```

### Disaster Recovery

Complete system recovery procedure:

1. **Flash fresh Raspberry Pi OS**

```bash
# On another computer, flash SD card
# Use Raspberry Pi Imager
```

2. **Basic system setup**

```bash
# SSH in, update system
sudo apt update && sudo apt upgrade -y
```

3. **Restore from full image** (if available)

```bash
# On another system
gunzip -c pisovereign-full-20260207.img.gz | sudo dd of=/dev/mmcblk0 bs=4M status=progress
```

4. **Or restore components**

```bash
# Install PiSovereign
# (Follow installation guide)

# Restore configuration
sudo mkdir -p /etc/pisovereign
sudo cp config.toml.backup /etc/pisovereign/config.toml

# Restore database
pisovereign-cli restore --input pisovereign-backup.db

# Restore Vault (if using local Vault)
sudo tar -xzf vault-backup.tar.gz -C /opt/vault/

# Start services
sudo systemctl start pisovereign
```

---

## Backup Verification

### Verify Database Backup

```bash
# Check file integrity
gzip -t /backup/pisovereign/daily/pisovereign-20260207.db.gz && echo "OK"

# Test restore to temp location
gunzip -c /backup/pisovereign/daily/pisovereign-20260207.db.gz > /tmp/test.db
sqlite3 /tmp/test.db "PRAGMA integrity_check;"
sqlite3 /tmp/test.db "SELECT COUNT(*) FROM conversations;"
rm /tmp/test.db
```

### Automated Verification

```bash
#!/bin/bash
# /usr/local/bin/verify-backup.sh

BACKUP_FILE="/backup/pisovereign/daily/pisovereign-$(date +%Y%m%d).db.gz"

if [ ! -f "$BACKUP_FILE" ]; then
    echo "ERROR: Today's backup not found!"
    exit 1
fi

# Verify gzip integrity
if ! gzip -t "$BACKUP_FILE" 2>/dev/null; then
    echo "ERROR: Backup file is corrupted!"
    exit 1
fi

# Verify database integrity
gunzip -c "$BACKUP_FILE" > /tmp/verify.db
INTEGRITY=$(sqlite3 /tmp/verify.db "PRAGMA integrity_check;" 2>&1)
rm /tmp/verify.db

if [ "$INTEGRITY" != "ok" ]; then
    echo "ERROR: Database integrity check failed: $INTEGRITY"
    exit 1
fi

echo "Backup verification passed"
```

Add to cron:

```cron
# Verify backup at 3 AM (after 2 AM backup)
0 3 * * * /usr/local/bin/verify-backup.sh || echo "Backup verification failed!" | mail -s "PiSovereign Backup Alert" admin@example.com
```

---

## Retention Policy

### Recommended Policy

| Type | Retention | Storage Estimate |
|------|-----------|------------------|
| Daily | 7 days | ~70 MB |
| Weekly | 4 weeks | ~40 MB |
| Monthly | 12 months | ~120 MB |
| **Total** | - | **~230 MB** |

### Cleanup Script

```bash
#!/bin/bash
# /usr/local/bin/cleanup-backups.sh

BACKUP_DIR="/backup/pisovereign"

# Remove old daily backups (older than 7 days)
find "$BACKUP_DIR/daily" -name "*.db.gz" -mtime +7 -delete

# Remove old weekly backups (older than 28 days)
find "$BACKUP_DIR/weekly" -name "*.db.gz" -mtime +28 -delete

# Remove old monthly backups (older than 365 days)
find "$BACKUP_DIR/monthly" -name "*.db.gz" -mtime +365 -delete

# Remove old config backups (older than 30 days)
find "$BACKUP_DIR/config" -name "*.toml" -mtime +30 -delete

# Report disk usage
echo "Backup disk usage:"
du -sh "$BACKUP_DIR"/*
```

---

## Quick Reference

### Backup Commands

```bash
# Local backup
pisovereign-cli backup --output /backup/db.db

# S3 backup
pisovereign-cli backup --s3-bucket mybucket --s3-prefix daily/

# Verify backup
sqlite3 backup.db "PRAGMA integrity_check;"
```

### Restore Commands

```bash
# Local restore
pisovereign-cli restore --input /backup/db.db

# S3 restore
pisovereign-cli restore --s3-bucket mybucket --s3-key daily/db.db
```

### Monitoring Backup Health

Add to Prometheus:

```yaml
# prometheus/rules/backups.yml
groups:
  - name: backups
    rules:
      - alert: BackupMissing
        expr: time() - file_mtime{path="/backup/pisovereign/daily/latest.db.gz"} > 86400
        for: 1h
        labels:
          severity: warning
        annotations:
          summary: "Daily backup is missing"
          description: "No backup created in the last 24 hours"
```

---

## Next Steps

- [Security Hardening](../security/hardening.md) - Encrypt backups
- [Monitoring](./monitoring.md) - Monitor backup health
