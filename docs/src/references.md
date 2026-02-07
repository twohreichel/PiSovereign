# References

> 📚 External resources and documentation references

This page collects official documentation, tutorials, and resources referenced throughout the PiSovereign documentation.

---

## Hardware

### Raspberry Pi 5

| Resource | Description |
|----------|-------------|
| [Raspberry Pi 5 Product Page](https://www.raspberrypi.com/products/raspberry-pi-5/) | Official product information |
| [Raspberry Pi 5 Documentation](https://www.raspberrypi.com/documentation/computers/raspberry-pi-5.html) | Hardware specifications and setup |
| [Raspberry Pi OS](https://www.raspberrypi.com/software/) | Operating system downloads |
| [Raspberry Pi Imager](https://www.raspberrypi.com/software/) | SD card flashing tool |
| [GPIO Pinout](https://pinout.xyz/) | Interactive pinout reference |

### Hailo AI Accelerator

| Resource | Description |
|----------|-------------|
| [Hailo-10H AI HAT+ Product Page](https://hailo.ai/products/hailo-8-m-2-ai-acceleration-modules/) | Official product information |
| [Hailo Developer Zone](https://hailo.ai/developer-zone/) | SDKs, tools, and documentation |
| [HailoRT SDK 4.20 Documentation](https://hailo.ai/developer-zone/documentation/) | Runtime SDK reference |
| [Hailo Model Zoo](https://hailo.ai/developer-zone/model-zoo/) | Pre-compiled models |
| [Hailo-Ollama GitHub](https://github.com/hailo-ai) | Ollama-compatible inference server |

### Storage

| Resource | Description |
|----------|-------------|
| [NVMe SSD Compatibility](https://www.raspberrypi.com/documentation/computers/raspberry-pi.html#nvme-ssd-boot) | NVMe boot support |
| [PCIe HAT+ Documentation](https://www.raspberrypi.com/documentation/accessories/pcie-hat.html) | PCIe expansion |

---

## Rust Ecosystem

### Language & Tools

| Resource | Description |
|----------|-------------|
| [The Rust Programming Language](https://doc.rust-lang.org/book/) | Official Rust book |
| [Rust by Example](https://doc.rust-lang.org/rust-by-example/) | Learn Rust through examples |
| [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) | Best practices for API design |
| [Rust Edition Guide](https://doc.rust-lang.org/edition-guide/) | Edition migration guide |
| [rustup Documentation](https://rust-lang.github.io/rustup/) | Toolchain manager |
| [Cargo Book](https://doc.rust-lang.org/cargo/) | Package manager documentation |

### Frameworks Used

| Resource | Description |
|----------|-------------|
| [Axum Documentation](https://docs.rs/axum/latest/axum/) | Web framework |
| [Tokio Documentation](https://tokio.rs/tokio/tutorial) | Async runtime |
| [SQLx Documentation](https://docs.rs/sqlx/latest/sqlx/) | Async SQL toolkit |
| [Serde Documentation](https://serde.rs/) | Serialization framework |
| [Tower Documentation](https://docs.rs/tower/latest/tower/) | Middleware framework |
| [Tracing Documentation](https://docs.rs/tracing/latest/tracing/) | Application instrumentation |
| [Clap Documentation](https://docs.rs/clap/latest/clap/) | Command-line parser |
| [Reqwest Documentation](https://docs.rs/reqwest/latest/reqwest/) | HTTP client |
| [Utoipa Documentation](https://docs.rs/utoipa/latest/utoipa/) | OpenAPI generation |

### Testing & Quality

| Resource | Description |
|----------|-------------|
| [Rust Testing](https://doc.rust-lang.org/book/ch11-00-testing.html) | Testing in Rust |
| [cargo-tarpaulin](https://github.com/xd009642/tarpaulin) | Code coverage tool |
| [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) | Dependency linting |
| [Clippy Lints](https://rust-lang.github.io/rust-clippy/master/index.html) | Lint reference |
| [Rustfmt Configuration](https://rust-lang.github.io/rustfmt/) | Formatter options |

---

## Security

### HashiCorp Vault

| Resource | Description |
|----------|-------------|
| [Vault Documentation](https://developer.hashicorp.com/vault/docs) | Official documentation |
| [Vault Getting Started](https://developer.hashicorp.com/vault/tutorials/getting-started) | Beginner tutorials |
| [KV Secrets Engine v2](https://developer.hashicorp.com/vault/docs/secrets/kv/kv-v2) | Key-value secrets |
| [AppRole Auth Method](https://developer.hashicorp.com/vault/docs/auth/approle) | Application authentication |
| [Vault Security Model](https://developer.hashicorp.com/vault/docs/internals/security) | Security architecture |
| [Vault Production Hardening](https://developer.hashicorp.com/vault/tutorials/operations/production-hardening) | Production best practices |

### System Security

| Resource | Description |
|----------|-------------|
| [CIS Benchmarks](https://www.cisecurity.org/cis-benchmarks) | Security configuration guides |
| [OWASP API Security Top 10](https://owasp.org/www-project-api-security/) | API security risks |
| [Mozilla SSL Configuration](https://ssl-config.mozilla.org/) | TLS configuration generator |
| [SSH Hardening Guide](https://www.ssh-audit.com/hardening_guides.html) | SSH security |
| [Fail2ban Documentation](https://www.fail2ban.org/wiki/index.php/Main_Page) | Intrusion prevention |

### Cryptography

| Resource | Description |
|----------|-------------|
| [RustCrypto](https://github.com/RustCrypto) | Pure Rust crypto implementations |
| [ring Documentation](https://briansmith.org/rustdoc/ring/) | Crypto library |
| [Argon2 Specification](https://www.rfc-editor.org/rfc/rfc9106.html) | Password hashing |

---

## APIs & Integrations

### AI & Language Models

| Resource | Description |
|----------|-------------|
| [OpenAI API Reference](https://platform.openai.com/docs/api-reference) | OpenAI API docs |
| [Ollama API](https://github.com/ollama/ollama/blob/main/docs/api.md) | Ollama REST API |
| [LLM Tokenization](https://huggingface.co/docs/transformers/tokenizer_summary) | Understanding tokenizers |

### Communication

| Resource | Description |
|----------|-------------|
| [WhatsApp Business API](https://developers.facebook.com/docs/whatsapp/cloud-api) | WhatsApp Cloud API |
| [WhatsApp Webhooks](https://developers.facebook.com/docs/whatsapp/cloud-api/webhooks) | Webhook setup |

### Email

| Resource | Description |
|----------|-------------|
| [Proton Bridge](https://proton.me/mail/bridge) | Proton Mail IMAP/SMTP bridge |
| [IMAP RFC 3501](https://datatracker.ietf.org/doc/html/rfc3501) | IMAP protocol |
| [SMTP RFC 5321](https://datatracker.ietf.org/doc/html/rfc5321) | SMTP protocol |

### Calendar

| Resource | Description |
|----------|-------------|
| [CalDAV RFC 4791](https://datatracker.ietf.org/doc/html/rfc4791) | CalDAV protocol |
| [iCalendar RFC 5545](https://datatracker.ietf.org/doc/html/rfc5545) | iCalendar format |
| [Baïkal Server](https://sabre.io/baikal/) | CalDAV/CardDAV server |

### Weather

| Resource | Description |
|----------|-------------|
| [Open-Meteo API](https://open-meteo.com/en/docs) | Free weather API |

---

## Infrastructure

### Docker

| Resource | Description |
|----------|-------------|
| [Docker Documentation](https://docs.docker.com/) | Official docs |
| [Docker Compose](https://docs.docker.com/compose/) | Multi-container apps |
| [Docker on Raspberry Pi](https://docs.docker.com/engine/install/debian/) | ARM installation |

### Reverse Proxy

| Resource | Description |
|----------|-------------|
| [Traefik Documentation](https://doc.traefik.io/traefik/) | Cloud-native proxy |
| [Let's Encrypt](https://letsencrypt.org/docs/) | Free TLS certificates |
| [Nginx Documentation](https://nginx.org/en/docs/) | Web server/proxy |

### Monitoring

| Resource | Description |
|----------|-------------|
| [Prometheus Documentation](https://prometheus.io/docs/) | Metrics collection |
| [Grafana Documentation](https://grafana.com/docs/) | Visualization |
| [Loki Documentation](https://grafana.com/docs/loki/) | Log aggregation |
| [OpenTelemetry](https://opentelemetry.io/docs/) | Observability framework |

### Databases

| Resource | Description |
|----------|-------------|
| [SQLite Documentation](https://www.sqlite.org/docs.html) | Embedded database |
| [SQLite Performance](https://www.sqlite.org/fasterthanfs.html) | Optimization tips |

---

## Development Tools

### VS Code

| Resource | Description |
|----------|-------------|
| [rust-analyzer](https://rust-analyzer.github.io/) | Rust language server |
| [CodeLLDB](https://marketplace.visualstudio.com/items?itemName=vadimcn.vscode-lldb) | Debugger |
| [Even Better TOML](https://marketplace.visualstudio.com/items?itemName=tamasfe.even-better-toml) | TOML support |

### GitHub

| Resource | Description |
|----------|-------------|
| [GitHub Actions](https://docs.github.com/en/actions) | CI/CD platform |
| [Release Please](https://github.com/googleapis/release-please) | Release automation |
| [GitHub Pages](https://docs.github.com/en/pages) | Static site hosting |

### Documentation

| Resource | Description |
|----------|-------------|
| [mdBook Documentation](https://rust-lang.github.io/mdBook/) | Documentation tool |
| [rustdoc Book](https://doc.rust-lang.org/rustdoc/) | Rust documentation |

---

## Standards & Specifications

| Resource | Description |
|----------|-------------|
| [OpenAPI Specification](https://spec.openapis.org/oas/latest.html) | API description format |
| [JSON Schema](https://json-schema.org/) | JSON validation |
| [Semantic Versioning](https://semver.org/) | Version numbering |
| [Keep a Changelog](https://keepachangelog.com/) | Changelog format |
| [Conventional Commits](https://www.conventionalcommits.org/) | Commit message format |

---

## Community

| Resource | Description |
|----------|-------------|
| [Rust Users Forum](https://users.rust-lang.org/) | Community forum |
| [Rust Discord](https://discord.gg/rust-lang) | Chat community |
| [This Week in Rust](https://this-week-in-rust.org/) | Weekly newsletter |
| [Raspberry Pi Forums](https://forums.raspberrypi.com/) | Hardware community |

---

> 💡 **Tip**: Many of these resources are updated regularly. Always check for the latest version of documentation when implementing features.
