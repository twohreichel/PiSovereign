//! SQLite database backup functionality
//!
//! Provides online backup capabilities using SQLite's backup API,
//! with optional upload to S3-compatible storage.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::Utc;
use s3::creds::Credentials;
use s3::{Bucket, Region};
use tracing::{debug, info, warn};

/// Configuration for S3 backup destination
#[derive(Debug, Clone)]
pub struct S3Config {
    /// S3 bucket name
    pub bucket: String,
    /// S3 region (e.g., "us-east-1", "eu-central-1")
    pub region: String,
    /// Custom S3 endpoint (for MinIO, Backblaze B2, etc.)
    pub endpoint: Option<String>,
    /// S3 access key (from env if not provided)
    pub access_key: Option<String>,
    /// S3 secret key (from env if not provided)
    pub secret_key: Option<String>,
    /// Prefix path within the bucket
    pub prefix: Option<String>,
}

/// Result of a backup operation
#[derive(Debug)]
pub struct BackupResult {
    /// Path to the local backup file
    pub local_path: PathBuf,
    /// Size of the backup file in bytes
    pub size_bytes: u64,
    /// Duration of the backup operation
    pub duration_ms: u64,
    /// S3 URL if uploaded
    pub s3_url: Option<String>,
}

/// Perform an online backup of the SQLite database
///
/// Uses SQLite's backup API to create a consistent snapshot while
/// the database is in use. The backup is atomic and does not block
/// normal database operations.
///
/// # Arguments
///
/// * `source_db_path` - Path to the source SQLite database
/// * `output_path` - Optional output path (auto-generated if None)
/// * `s3_config` - Optional S3 configuration for remote upload
///
/// # Returns
///
/// A `BackupResult` containing the local path, size, and optional S3 URL.
///
/// # Errors
///
/// Returns an error if:
/// - The source database cannot be opened
/// - The backup operation fails
/// - S3 upload fails (when S3 config is provided)
pub async fn backup_database(
    source_db_path: &Path,
    output_path: Option<PathBuf>,
    s3_config: Option<S3Config>,
) -> Result<BackupResult> {
    let start = Instant::now();

    // Generate output filename if not provided
    let backup_path = output_path.unwrap_or_else(|| {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("pisovereign_backup_{timestamp}.db");
        PathBuf::from(&filename)
    });

    info!(
        source = %source_db_path.display(),
        destination = %backup_path.display(),
        "Starting SQLite online backup"
    );

    // Perform the SQLite backup
    perform_sqlite_backup(source_db_path, &backup_path)
        .await
        .context("SQLite backup failed")?;

    // Get backup file size
    let metadata = tokio::fs::metadata(&backup_path)
        .await
        .context("Failed to get backup file metadata")?;
    let size_bytes = metadata.len();

    info!(size_bytes = size_bytes, "Backup completed locally");

    // Upload to S3 if configured
    let s3_url = if let Some(config) = s3_config {
        Some(
            upload_to_s3(&backup_path, &config)
                .await
                .context("S3 upload failed")?,
        )
    } else {
        None
    };

    // Duration in milliseconds (u128 -> u64 truncation is acceptable for practical durations)
    #[allow(clippy::cast_possible_truncation)]
    let duration_ms = start.elapsed().as_millis() as u64;

    Ok(BackupResult {
        local_path: backup_path,
        size_bytes,
        duration_ms,
        s3_url,
    })
}

/// Perform SQLite backup using the backup API
///
/// This runs in a blocking task to avoid blocking the async runtime.
async fn perform_sqlite_backup(source_path: &Path, dest_path: &Path) -> Result<()> {
    let source_path = source_path.to_path_buf();
    let dest_path = dest_path.to_path_buf();

    tokio::task::spawn_blocking(move || {
        use rusqlite::{Connection, backup};

        // Open source database in read-only mode
        let source_conn = Connection::open_with_flags(
            &source_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .context("Failed to open source database")?;

        // Create/open destination database
        let mut dest_conn = Connection::open(&dest_path).context("Failed to create backup file")?;

        // Perform the backup
        {
            let backup = backup::Backup::new(&source_conn, &mut dest_conn)
                .context("Failed to initialize backup")?;

            // Copy all pages (-1 = copy all at once)
            // For very large databases, you might want to use a smaller page count
            // and call step() in a loop with progress reporting
            backup.step(-1).context("Backup step failed")?;

            debug!("Backup completed successfully");
            // backup is dropped here, releasing the borrow
        }

        // Ensure destination is properly closed and synced
        dest_conn
            .close()
            .map_err(|(_, e)| e)
            .context("Failed to close backup file")?;

        Ok(())
    })
    .await
    .context("Backup task panicked")?
}

/// Upload backup file to S3-compatible storage
async fn upload_to_s3(local_path: &Path, config: &S3Config) -> Result<String> {
    info!(
        bucket = %config.bucket,
        endpoint = ?config.endpoint,
        "Uploading backup to S3"
    );

    // Build credentials
    let credentials = if config.access_key.is_some() && config.secret_key.is_some() {
        Credentials::new(
            config.access_key.as_deref(),
            config.secret_key.as_deref(),
            None,
            None,
            None,
        )
        .context("Failed to create S3 credentials")?
    } else {
        // Try to load from environment
        Credentials::default().context(
            "Failed to load S3 credentials from environment. \
             Set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY",
        )?
    };

    // Build region
    let region = if let Some(ref endpoint) = config.endpoint {
        Region::Custom {
            region: config.region.clone(),
            endpoint: endpoint.clone(),
        }
    } else {
        config.region.parse().context("Invalid S3 region")?
    };

    // Create bucket handle
    let bucket = Bucket::new(&config.bucket, region, credentials)
        .context("Failed to create S3 bucket handle")?
        .with_path_style(); // Required for MinIO and some S3-compatible services

    // Generate S3 key
    let filename = local_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("backup.db");
    let s3_key = config.prefix.as_ref().map_or_else(
        || filename.to_string(),
        |prefix| format!("{prefix}/{filename}"),
    );

    // Read file
    let content = tokio::fs::read(local_path)
        .await
        .context("Failed to read backup file")?;

    // Upload
    let response = bucket
        .put_object(&s3_key, &content)
        .await
        .context("S3 upload failed")?;

    if response.status_code() != 200 {
        anyhow::bail!(
            "S3 upload returned status {}: {}",
            response.status_code(),
            String::from_utf8_lossy(response.as_slice())
        );
    }

    let s3_url = format!("s3://{}/{}", config.bucket, s3_key);
    info!(url = %s3_url, "Backup uploaded to S3");

    Ok(s3_url)
}

/// Delete old local backups, keeping the most recent N
pub async fn cleanup_old_backups(backup_dir: &Path, keep_count: usize) -> Result<usize> {
    let mut backups: Vec<_> = Vec::new();

    let mut entries = tokio::fs::read_dir(backup_dir)
        .await
        .context("Failed to read backup directory")?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "db") {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("pisovereign_backup_") {
                    let metadata = entry.metadata().await?;
                    backups.push((path, metadata.modified()?));
                }
            }
        }
    }

    // Sort by modification time, newest first
    backups.sort_by(|a, b| b.1.cmp(&a.1));

    // Delete old backups
    let mut deleted = 0;
    for (path, _) in backups.into_iter().skip(keep_count) {
        match tokio::fs::remove_file(&path).await {
            Ok(()) => {
                info!(path = %path.display(), "Deleted old backup");
                deleted += 1;
            },
            Err(e) => {
                warn!(path = %path.display(), error = %e, "Failed to delete old backup");
            },
        }
    }

    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_backup_creates_file() {
        let temp_dir = TempDir::new().unwrap();
        let source_path = temp_dir.path().join("source.db");
        let backup_path = temp_dir.path().join("backup.db");

        // Create a test database
        {
            let conn = rusqlite::Connection::open(&source_path).unwrap();
            conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)", [])
                .unwrap();
            conn.execute("INSERT INTO test (value) VALUES ('hello')", [])
                .unwrap();
        }

        // Perform backup
        let result = backup_database(&source_path, Some(backup_path.clone()), None)
            .await
            .unwrap();

        // Verify backup file exists
        assert!(result.local_path.exists());
        assert!(result.size_bytes > 0);
        assert!(result.s3_url.is_none());

        // Verify backup contains data
        let backup_conn = rusqlite::Connection::open(&backup_path).unwrap();
        let value: String = backup_conn
            .query_row("SELECT value FROM test WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "hello");
    }

    #[tokio::test]
    async fn test_backup_generates_filename() {
        let temp_dir = TempDir::new().unwrap();
        let source_path = temp_dir.path().join("source.db");

        // Create a test database
        rusqlite::Connection::open(&source_path).unwrap();

        // Change to temp dir for auto-generated filename
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = backup_database(&source_path, None, None).await.unwrap();

        std::env::set_current_dir(original_dir).unwrap();

        assert!(
            result
                .local_path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("pisovereign_backup_")
        );
    }

    #[tokio::test]
    async fn test_cleanup_keeps_recent_backups() {
        let temp_dir = TempDir::new().unwrap();

        // Create some fake backup files with different timestamps
        for i in 0..5 {
            let path = temp_dir
                .path()
                .join(format!("pisovereign_backup_2024010{i}_120000.db"));
            tokio::fs::write(&path, b"test").await.unwrap();
            // Add small delay to ensure different modification times
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        // Keep only 2 most recent
        let deleted = cleanup_old_backups(temp_dir.path(), 2).await.unwrap();

        assert_eq!(deleted, 3);

        // Count remaining files
        let mut count = 0;
        let mut entries = tokio::fs::read_dir(temp_dir.path()).await.unwrap();
        while entries.next_entry().await.unwrap().is_some() {
            count += 1;
        }
        assert_eq!(count, 2);
    }
}
