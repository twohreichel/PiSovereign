# Contributing

> 🤝 Guidelines for contributing to PiSovereign

Thank you for your interest in contributing to PiSovereign! This guide will help you get started.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Development Setup](#development-setup)
  - [Prerequisites](#prerequisites)
  - [Environment Setup](#environment-setup)
  - [Running Tests](#running-tests)
- [Code Style](#code-style)
  - [Rust Formatting](#rust-formatting)
  - [Commit Messages](#commit-messages)
  - [Documentation](#documentation)
- [Pull Request Process](#pull-request-process)
  - [Before You Start](#before-you-start)
  - [Creating a PR](#creating-a-pr)
  - [Review Process](#review-process)
- [Development Workflow](#development-workflow)

---

## Code of Conduct

This project adheres to a Code of Conduct. By participating, you are expected to:

- Be respectful and inclusive
- Accept constructive criticism gracefully
- Focus on what's best for the community
- Show empathy towards others

---

## Development Setup

### Prerequisites

| Requirement | Version | Notes |
|-------------|---------|-------|
| **Rust** | 1.93.0+ | Edition 2024 |
| **Just** | Latest | Command runner |
| **SQLite** | 3.x | Development database |
| **FFmpeg** | 5.x+ | Audio processing |

### Environment Setup

1. **Clone the repository**

```bash
git clone https://github.com/twohreichel/PiSovereign.git
cd PiSovereign
```

2. **Install Rust toolchain**

```bash
# Install rustup if needed
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install required components
rustup component add rustfmt clippy

# Install nightly for docs (optional)
rustup toolchain install nightly
```

3. **Install Just**

```bash
# macOS
brew install just

# Linux
cargo install just
```

4. **Install development dependencies**

```bash
# macOS
brew install sqlite ffmpeg

# Ubuntu/Debian
sudo apt install libsqlite3-dev ffmpeg pkg-config libssl-dev
```

5. **Verify setup**

```bash
# Run quality checks
just quality

# Build the project
just build
```

### Running Tests

```bash
# Run all tests
just test

# Run tests with output
just test-verbose

# Run specific crate tests
cargo test -p domain
cargo test -p application

# Run integration tests
cargo test --test '*' -- --ignored

# Generate coverage report
just coverage
```

---

## Code Style

### Rust Formatting

We use `rustfmt` with custom configuration:

```bash
# Format all code
just fmt

# Check formatting (CI will fail if not formatted)
just fmt-check
```

Configuration in `rustfmt.toml`:

```toml
edition = "2024"
max_width = 100
use_small_heuristics = "Default"
imports_granularity = "Crate"
group_imports = "StdExternalCrate"
```

### Clippy Lints

We enforce strict Clippy lints:

```bash
# Run clippy
just lint

# Auto-fix issues
just lint-fix
```

Key lint categories enabled:
- `clippy::pedantic` - Strict lints
- `clippy::nursery` - Experimental but useful lints
- `clippy::cargo` - Cargo.toml best practices

### Commit Messages

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

**Types:**
| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation only |
| `style` | Code style (formatting, no logic change) |
| `refactor` | Code change that neither fixes nor adds |
| `perf` | Performance improvement |
| `test` | Adding or updating tests |
| `chore` | Maintenance tasks |

**Examples:**

```
feat(api): add streaming chat endpoint

Implements SSE-based streaming for /v1/chat/stream endpoint.
Supports token-by-token response streaming for better UX.

Closes #123
```

```
fix(inference): handle timeout gracefully

Previously, inference timeouts caused a panic. Now returns
a proper error response with retry information.
```

### Documentation

All public APIs must be documented:

```rust
/// Processes a user message and returns an AI response.
///
/// This method handles the full conversation flow including:
/// - Loading conversation context
/// - Calling the inference engine
/// - Persisting the response
///
/// # Arguments
///
/// * `conversation_id` - Optional ID to continue existing conversation
/// * `message` - The user's message content
///
/// # Returns
///
/// Returns the AI's response or an error if processing fails.
///
/// # Errors
///
/// - `ServiceError::Inference` - If the inference engine is unavailable
/// - `ServiceError::Database` - If conversation persistence fails
///
/// # Examples
///
/// ```rust,ignore
/// let response = service.send_message(
///     Some(conversation_id),
///     "What's the weather?".to_string(),
/// ).await?;
/// ```
pub async fn send_message(
    &self,
    conversation_id: Option<ConversationId>,
    message: String,
) -> Result<Message, ServiceError> {
    // ...
}
```

---

## Pull Request Process

### Before You Start

1. **Check existing issues/PRs**
   - Look for related issues or PRs
   - Comment on the issue you want to work on

2. **Create an issue first** (for features)
   - Describe the feature
   - Discuss approach before implementing

3. **Fork and branch**
   ```bash
   git checkout -b feat/my-feature
   # or
   git checkout -b fix/issue-123
   ```

### Creating a PR

1. **Ensure quality checks pass**
   ```bash
   just pre-commit
   ```

2. **Write/update tests**
   - Add tests for new functionality
   - Ensure existing tests still pass

3. **Update documentation**
   - Update relevant docs in `docs/`
   - Add doc comments to new public APIs

4. **Push and create PR**
   ```bash
   git push origin feat/my-feature
   ```

5. **Fill out PR template**
   - Description of changes
   - Related issues
   - Testing performed
   - Breaking changes (if any)

### PR Template

```markdown
## Description
Brief description of what this PR does.

## Related Issues
Fixes #123
Related to #456

## Type of Change
- [ ] Bug fix (non-breaking)
- [ ] New feature (non-breaking)
- [ ] Breaking change
- [ ] Documentation update

## Testing
- [ ] Unit tests added/updated
- [ ] Integration tests added/updated
- [ ] Manually tested on Raspberry Pi

## Checklist
- [ ] Code follows project style guidelines
- [ ] Self-review completed
- [ ] Documentation updated
- [ ] No new warnings
```

### Review Process

1. **Automated checks** must pass:
   - Format check (`rustfmt`)
   - Lint check (`clippy`)
   - Tests (all platforms)
   - Coverage (no significant decrease)
   - Security scan (`cargo-deny`)

2. **Human review**:
   - At least one maintainer approval required
   - Address all review comments

3. **Merge**:
   - Squash and merge for clean history
   - Delete branch after merge

---

## Development Workflow

### Common Tasks

```bash
# Full quality check (run before pushing)
just quality

# Quick pre-commit check
just pre-commit

# Run the server locally
just run

# Run CLI commands
just cli status
just cli chat "Hello"

# Generate and view documentation
just docs

# Clean build artifacts
just clean
```

### Project Structure

```
PiSovereign/
├── crates/                 # Rust crates
│   ├── domain/            # Core business logic
│   ├── application/       # Use cases, services
│   ├── infrastructure/    # External adapters
│   ├── ai_core/          # Inference engine
│   ├── ai_speech/        # Speech processing
│   ├── integration_*/    # Service integrations
│   └── presentation_*/   # HTTP API, CLI
├── docs/                  # mdBook documentation
├── grafana/              # Monitoring configuration
├── migrations/           # Database migrations
└── .github/              # CI/CD workflows
```

### Adding a New Feature

1. **Domain layer** (if new entities/values needed)
   ```bash
   # Edit crates/domain/src/entities/mod.rs
   # Add new entity module
   ```

2. **Application layer** (service logic)
   ```bash
   # Add port trait in crates/application/src/ports/
   # Add service in crates/application/src/services/
   ```

3. **Infrastructure layer** (adapters)
   ```bash
   # Implement port in crates/infrastructure/src/adapters/
   ```

4. **Presentation layer** (API endpoints)
   ```bash
   # Add handler in crates/presentation_http/src/handlers/
   # Add route in crates/presentation_http/src/router.rs
   ```

5. **Tests**
   ```bash
   # Unit tests alongside code
   # Integration tests in crates/*/tests/
   ```

### Database Migrations

```bash
# Create new migration
cat > migrations/V007__my_migration.sql << 'EOF'
-- Description of migration
CREATE TABLE my_table (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
EOF

# Migrations run automatically on startup (if enabled)
# Or manually:
pisovereign-cli migrate
```

---

## Getting Help

- **Questions**: Use [GitHub Discussions](https://github.com/twohreichel/PiSovereign/discussions)
- **Bugs**: Open an [Issue](https://github.com/twohreichel/PiSovereign/issues)
- **Security**: Report via GitHub Security Advisories

Thank you for contributing! 🎉
