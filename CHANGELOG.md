# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **BREAKING**: Upgraded Rust toolchain from `nightly-2025-01-15` to `stable 1.93.0`
- **BREAKING**: Migrated from Edition 2021 to Edition 2024
- **BREAKING**: Replaced `SledCache` with `RedbCache` for L2 caching
  - The `sled` database (0.34) was unmaintained and has been replaced with `redb` (2.6)
  - A deprecated type alias `SledCache = RedbCache` is provided for migration
  - Database files are **not** compatible; existing cache will be cleared on first start
- **BREAKING**: Upgraded `bincode` from 1.3 to 2.0
  - New API uses `encode_to_vec`/`decode_from_slice` instead of `serialize`/`deserialize`
  - Requires `bincode::Encode` and `bincode::Decode` derives on cached types

### Migration Guide

#### Rust Toolchain

Update your `rust-toolchain.toml` or ensure you have Rust 1.93.0+ installed:

```toml
[toolchain]
channel = "1.93.0"
```

#### Cache Migration

If you were using `SledCache` directly:

```rust
// Before
use infrastructure::cache::SledCache;
let cache = SledCache::new("path/to/cache")?;

// After
use infrastructure::cache::RedbCache;
let cache = RedbCache::new("path/to/cache")?;
```

**Note**: Existing sled database files are not compatible with redb. The cache will
start fresh after migration. If you have critical cached data, export it before
upgrading.

#### Bincode Serialization

If you have custom types stored in the cache:

```rust
// Before (bincode 1.x)
#[derive(Serialize, Deserialize)]
struct MyCachedData {
    field: String,
}

// After (bincode 2.x)
use bincode::{Encode, Decode};

#[derive(Serialize, Deserialize, Encode, Decode)]
struct MyCachedData {
    field: String,
}
```

### Added

- GitHub Actions CI/CD pipeline with:
  - Formatting checks (`cargo fmt`)
  - Linting (`cargo clippy`)
  - Test execution
  - Code coverage reporting
- Dependabot configuration for automated dependency updates
- `RedbCache` implementation with:
  - Automatic database recovery for corrupted files
  - In-memory mode for testing
  - Full compatibility with `CachePort` trait

### Fixed

- Added missing `serialize` feature to `quick-xml` dependency in `integration_caldav`

### Security

- Replaced unmaintained `sled` database with actively maintained `redb`
- Updated all dependencies to latest versions

## [0.1.0] - Initial Release

### Added

- Domain-driven architecture with clean separation of concerns
- AI-powered chat service with conversation history
- Email integration via Proton Bridge (IMAP/SMTP)
- Calendar integration via CalDAV
- WhatsApp Business API integration
- Multi-layer caching (Moka L1 + persistent L2)
- Approval workflow for sensitive operations
- Audit logging with SQLite persistence
- HTTP API with Axum web framework
- CLI interface for local interaction
- Rate limiting and authentication middleware
- Circuit breaker pattern for external services
- Prometheus metrics and Grafana dashboards

[Unreleased]: https://github.com/twohreichel/PiSovereign/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/twohreichel/PiSovereign/releases/tag/v0.1.0
