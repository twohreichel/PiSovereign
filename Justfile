# PiSovereign Development Commands
# Install just: cargo install just
# Run: just <command>

# Default command - show available commands
default:
    @just --list

# === LINTING ===

# Run all lints (format check + clippy)
lint: lint-fmt lint-clippy
    @echo "✅ All lints passed!"

# Check code formatting without changes
lint-fmt:
    @echo "🔍 Checking formatting..."
    cargo fmt --all -- --check

# Run clippy with strict settings
lint-clippy:
    @echo "🔍 Running clippy..."
    cargo clippy --workspace --all-targets --all-features -- \
        -D warnings \
        -D clippy::all \
        -W clippy::pedantic \
        -W clippy::nursery \
        -A clippy::module_name_repetitions \
        -A clippy::must_use_candidate \
        -A clippy::missing_errors_doc \
        -A clippy::missing_panics_doc \
        -A clippy::doc_markdown \
        -A clippy::redundant_pub_crate \
        -A clippy::future_not_send \
        -A clippy::option_if_let_else \
        -A clippy::return_self_not_must_use \
        -A clippy::use_self \
        -A clippy::uninlined_format_args \
        -A clippy::derive_partial_eq_without_eq \
        -A clippy::unnested_or_patterns \
        -A clippy::literal_string_with_formatting_args \
        -A clippy::significant_drop_tightening \
        -A clippy::format_push_string \
        -A clippy::missing_const_for_fn

# Run clippy with auto-fix (applies safe fixes)
lint-fix:
    @echo "🔧 Applying clippy fixes..."
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged -- \
        -D warnings \
        -A clippy::module_name_repetitions \
        -A clippy::must_use_candidate

# === FORMATTING ===

# Format all code
fmt:
    @echo "✨ Formatting code..."
    cargo fmt --all

# Format and show diff
fmt-diff:
    cargo fmt --all -- --emit files
    git diff

# === TESTING ===

# Run all tests
test:
    @echo "🧪 Running tests..."
    cargo test --workspace

# Run tests with output
test-verbose:
    cargo test --workspace -- --nocapture

# === BUILD ===

# Build debug
build:
    cargo build --workspace

# Build release
build-release:
    cargo build --workspace --release

# Check without building
check:
    cargo check --workspace --all-targets

# === QUALITY ===

# Full quality check (lint + test + check)
quality: lint test check
    @echo "✅ All quality checks passed!"

# Pre-commit check (fast)
pre-commit: lint-fmt lint-clippy test
    @echo "✅ Ready to commit!"

# === CLEAN ===

# Clean build artifacts
clean:
    cargo clean

# === DOCS ===

# Generate and open documentation
docs:
    cargo doc --workspace --no-deps --open

# === RUN ===

# Run the server
run:
    cargo run --bin pisovereign-server

# Run the CLI
cli *ARGS:
    cargo run --bin pisovereign-cli -- {{ARGS}}

# === DOCKER ===

# Start all core services (production)
docker-up:
    cd docker && docker compose up -d

# Start with monitoring profile
docker-up-monitoring:
    cd docker && docker compose --profile monitoring up -d

# Start with CalDAV profile
docker-up-caldav:
    cd docker && docker compose --profile caldav up -d

# Start all profiles
docker-up-all:
    cd docker && docker compose --profile monitoring --profile caldav up -d

# Stop all services
docker-down:
    cd docker && docker compose --profile monitoring --profile caldav down

# Show service status
docker-status:
    cd docker && docker compose ps

# Follow logs for all services
docker-logs *ARGS:
    cd docker && docker compose logs -f {{ARGS}}

# Rebuild and restart PiSovereign container
docker-rebuild:
    cd docker && docker compose up -d --build pisovereign

# Initialize Vault (run once after first start)
docker-vault-init:
    cd docker && docker compose exec vault /vault/init.sh

# Unseal Vault manually
docker-vault-unseal KEY:
    cd docker && docker compose exec vault vault operator unseal {{KEY}}

# Show Vault status
docker-vault-status:
    cd docker && docker compose exec vault vault status

# Pull latest model into Ollama
docker-model-pull MODEL="qwen2.5:1.5b":
    cd docker && docker compose exec ollama ollama pull {{MODEL}}

# === COVERAGE ===
# Uses .tarpaulin.toml for configuration (75% threshold, excludes infrastructure code)

# Common exclusion patterns for infrastructure code tested via integration tests
_coverage_excludes := "--exclude-files '*/main.rs' --exclude-files '*/benches/*' --exclude-files '*/adapters/*' --exclude-files '*/persistence/*' --exclude-files '*/testing/*' --exclude-files '*/telemetry/*' --exclude-files '*/handlers/*' --exclude-files '*/config_reload.rs' --exclude-files '*/state.rs' --exclude-files '*/middleware/request_id.rs' --exclude-files '*/task.rs' --exclude-files '*/imap_client.rs' --exclude-files '*/smtp_client.rs'"

# Generate code coverage report (requires cargo-tarpaulin)
# Install with: cargo install cargo-tarpaulin
coverage:
    @echo "📊 Generating coverage report..."
    cargo tarpaulin --config .tarpaulin.toml {{ _coverage_excludes }}
    @echo "📊 Report generated at target/coverage/tarpaulin-report.html"

# Generate coverage report and open in browser
coverage-open:
    cargo tarpaulin --config .tarpaulin.toml {{ _coverage_excludes }}
    open target/coverage/tarpaulin-report.html

# Generate LCOV format only (for CI)
coverage-lcov:
    cargo tarpaulin --config .tarpaulin.toml --out Lcov {{ _coverage_excludes }}
    @echo "📊 LCOV report generated at target/coverage/lcov.info"

# Show coverage summary in terminal (no HTML)
coverage-summary:
    cargo tarpaulin --config .tarpaulin.toml --out Stdout {{ _coverage_excludes }}
