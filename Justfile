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
        -A clippy::format_push_string

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

# === COVERAGE ===

# Generate code coverage report (requires cargo-tarpaulin)
# Install with: cargo install cargo-tarpaulin
coverage:
    @echo "📊 Generating coverage report..."
    cargo tarpaulin --all-features --workspace --out Html --output-dir target/coverage
    @echo "📊 Report generated at target/coverage/tarpaulin-report.html"

# Generate coverage report and open in browser
coverage-open:
    cargo tarpaulin --all-features --workspace --out Html --output-dir target/coverage
    open target/coverage/tarpaulin-report.html

# Generate LCOV format for CI/Codecov
coverage-lcov:
    cargo tarpaulin --all-features --workspace --out Lcov --output-dir target/coverage
    @echo "📊 LCOV report generated at target/coverage/lcov.info"

# Show coverage summary in terminal
coverage-summary:
    cargo tarpaulin --all-features --workspace
