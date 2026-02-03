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
        -D clippy::pedantic \
        -D clippy::nursery \
        -A clippy::module_name_repetitions \
        -A clippy::must_use_candidate \
        -A clippy::missing_errors_doc \
        -A clippy::missing_panics_doc \
        -A clippy::doc_markdown \
        -A clippy::redundant_pub_crate \
        -A clippy::future_not_send

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
