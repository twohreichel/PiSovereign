# PiSovereign Documentation

> 🤖 **Local, secure AI assistant platform for Raspberry Pi 5 + Hailo-10H or macOS with Metal**

Welcome to the official PiSovereign documentation. This guide covers everything from initial hardware setup to production deployment and ongoing operations.

## Table of Contents

- [Introduction](#introduction)
- [Key Features](#key-features)
- [Quick Links](#quick-links)
- [Documentation Overview](#documentation-overview)
- [Getting Help](#getting-help)

---

## Introduction

PiSovereign is a privacy-focused AI assistant platform designed to run entirely on your own hardware. It supports two deployment targets:

- **Raspberry Pi 5 + Hailo-10H**: Dedicated AI appliance with NPU acceleration
- **macOS (Intel/Apple Silicon)**: Development or personal use with Metal GPU

Both platforms use the same Ollama-compatible API, ensuring identical functionality.

**Core Principles:**

- **Privacy First**: All processing happens locally on your device
- **EU/GDPR Compliant**: European services, no data leaves your network
- **Open Source**: MIT licensed, fully auditable code
- **Extensible**: Clean architecture enables easy customization

---

## Key Features

| Feature | Description |
|---------|-------------|
| 🧠 **Local LLM Inference** | Run Qwen2.5-1.5B on Hailo NPU or Ollama with Metal |
| 🍎 **Multi-Platform** | Raspberry Pi (production) or macOS (development) |
| 📱 **WhatsApp Control** | Send commands and receive responses via WhatsApp |
| 🎤 **Voice Messages** | Speech-to-Text and Text-to-Speech (local or cloud) |
| 📅 **Calendar Integration** | CalDAV support (Baïkal, Radicale, Nextcloud) |
| 📧 **Email Integration** | Proton Mail via Bridge (IMAP/SMTP) |
| 🌤️ **Weather** | Open-Meteo API (free, no API key required) |
| 🔒 **Secret Management** | HashiCorp Vault integration |
| 📊 **Monitoring** | Prometheus metrics + Grafana dashboards |

---

## Quick Links

### For Users

| Document | Description |
|----------|-------------|
| [Getting Started](./user/getting-started.md) | First-time setup guide |
| [Raspberry Pi Setup](./user/raspberry-pi-setup.md) | Complete hardware and OS configuration |
| [macOS Setup](./user/mac-setup.md) | Installation guide for Mac |
| [Vault Setup](./user/vault-setup.md) | HashiCorp Vault installation and integration |
| [Configuration](./user/configuration.md) | All `config.toml` options explained |
| [External Services](./user/external-services.md) | WhatsApp, Proton Mail, CalDAV setup |
| [Troubleshooting](./user/troubleshooting.md) | Common issues and solutions |

### For Developers

| Document | Description |
|----------|-------------|
| [Architecture](./developer/architecture.md) | System design and patterns |
| [Contributing](./developer/contributing.md) | How to contribute to PiSovereign |
| [Crate Reference](./developer/crate-reference.md) | Detailed documentation of all crates |
| [API Reference](./developer/api-reference.md) | REST API documentation with OpenAPI |

### For Operations

| Document | Description |
|----------|-------------|
| [Deployment](./operations/deployment.md) | Production deployment guide |
| [Monitoring](./operations/monitoring.md) | Prometheus, Grafana, and alerting |
| [Backup & Restore](./operations/backup-restore.md) | Data protection strategies |
| [Security Hardening](./security/hardening.md) | Complete security guide |

### API Documentation

| Resource | URL |
|----------|-----|
| **Cargo Docs (latest)** | [/api/latest/](../api/latest/presentation_http/index.html) |
| **OpenAPI Spec** | [/api/openapi.json](../api/openapi.json) |
| **Swagger UI** | [View in API Reference](./developer/api-reference.md#openapi-specification) |

---

## Documentation Overview

This documentation is organized into five main sections:

### 📘 User Guide

Step-by-step instructions for setting up and using PiSovereign. Start here if you're new to the project.

### 👩‍💻 Developer Guide

Technical documentation for contributors and developers who want to understand or extend PiSovereign.

### ⚙️ Operations

Guides for running PiSovereign in production, including deployment, monitoring, and maintenance.

### 🔐 Security

Comprehensive security hardening guide covering the Raspberry Pi, network configuration, Vault, and application security.

### 📚 References

External links to official documentation, standards, and resources used by PiSovereign.

---

## Getting Help

- **GitHub Issues**: [Report bugs or request features](https://github.com/twohreichel/PiSovereign/issues)
- **Discussions**: [Ask questions and share ideas](https://github.com/twohreichel/PiSovereign/discussions)
- **Security Issues**: Please report security vulnerabilities privately via GitHub Security Advisories

---

## Version Information

This documentation corresponds to the version shown in the version selector above. Use the dropdown to switch between documentation versions.

- **`latest`**: Points to the most recent stable release
- **`main`**: Development documentation (may include unreleased features)
- **`vX.Y.Z`**: Documentation for specific releases
