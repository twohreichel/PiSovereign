# Crate Reference

> 📦 Detailed documentation of all PiSovereign crates

This document provides comprehensive documentation for each crate in the PiSovereign workspace.

## Table of Contents

- [Overview](#overview)
- [Domain Layer](#domain-layer)
  - [domain](#domain)
- [Application Layer](#application-layer)
  - [application](#application)
- [Infrastructure Layer](#infrastructure-layer)
  - [infrastructure](#infrastructure)
- [AI Crates](#ai-crates)
  - [ai_core](#ai_core)
  - [ai_speech](#ai_speech)
- [Integration Crates](#integration-crates)
  - [integration_whatsapp](#integration_whatsapp)
  - [integration_proton](#integration_proton)
  - [integration_caldav](#integration_caldav)
  - [integration_weather](#integration_weather)
- [Presentation Crates](#presentation-crates)
  - [presentation_http](#presentation_http)
  - [presentation_cli](#presentation_cli)

---

## Overview

PiSovereign consists of 12 crates organized by architectural layer:

| Layer | Crates | Purpose |
|-------|--------|---------|
| Domain | `domain` | Core business logic, entities, value objects |
| Application | `application` | Use cases, services, port definitions |
| Infrastructure | `infrastructure` | Database, cache, secrets, telemetry |
| AI | `ai_core`, `ai_speech` | Inference engine, speech processing |
| Integration | `integration_*` | External service adapters |
| Presentation | `presentation_*` | HTTP API, CLI |

---

## Domain Layer

### domain

**Purpose**: Contains the core business logic with zero external dependencies (except `std`). Defines the ubiquitous language of the application.

**Dependencies**: None (pure Rust)

#### Entities

| Entity | Description |
|--------|-------------|
| `User` | Represents a system user with profile information |
| `Conversation` | A chat conversation containing messages |
| `Message` | A single message in a conversation |
| `ApprovalRequest` | Pending approval for sensitive operations |
| `AuditEntry` | Audit log entry for compliance |
| `CalendarEvent` | Calendar event representation |
| `EmailMessage` | Email representation |
| `WeatherData` | Weather information |

```rust
// Example: Conversation entity
pub struct Conversation {
    pub id: ConversationId,
    pub title: Option<String>,
    pub system_prompt: Option<String>,
    pub messages: Vec<Message>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

#### Value Objects

| Value Object | Description |
|--------------|-------------|
| `UserId` | Unique user identifier (UUID) |
| `ConversationId` | Unique conversation identifier |
| `MessageContent` | Validated message content |
| `TenantId` | Multi-tenant identifier |
| `PhoneNumber` | Validated phone number |

```rust
// Example: UserId value object
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserId(Uuid);

impl UserId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}
```

#### Commands

| Command | Description |
|---------|-------------|
| `UserCommand` | Commands from users (Briefing, Ask, Help, etc.) |
| `SystemCommand` | Internal system commands |

```rust
// User command variants
pub enum UserCommand {
    MorningBriefing,
    CreateCalendarEvent { title: String, start: DateTime<Utc>, end: DateTime<Utc> },
    SummarizeInbox { count: usize },
    DraftEmail { to: String, subject: String },
    SendEmail { draft_id: String },
    Ask { query: String },
    Echo { message: String },
    Help,
}
```

#### Domain Errors

```rust
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("Invalid message content: {0}")]
    InvalidContent(String),
    
    #[error("Conversation not found: {0}")]
    ConversationNotFound(ConversationId),
    
    #[error("User not authorized: {0}")]
    Unauthorized(String),
}
```

---

## Application Layer

### application

**Purpose**: Orchestrates use cases by coordinating domain entities and infrastructure through port interfaces.

**Dependencies**: `domain`

#### Services

| Service | Description |
|---------|-------------|
| `ConversationService` | Manages conversations and messages |
| `VoiceMessageService` | STT → LLM → TTS pipeline |
| `CommandService` | Parses and executes user commands |
| `ApprovalService` | Handles approval workflows |
| `BriefingService` | Generates morning briefings |
| `CalendarService` | Calendar operations |
| `EmailService` | Email operations |
| `HealthService` | System health checks |

```rust
// Example: ConversationService
pub struct ConversationService<R, I>
where
    R: ConversationRepository,
    I: InferencePort,
{
    repository: Arc<R>,
    inference: Arc<I>,
}

impl<R, I> ConversationService<R, I>
where
    R: ConversationRepository,
    I: InferencePort,
{
    pub async fn send_message(
        &self,
        conversation_id: Option<ConversationId>,
        content: String,
    ) -> Result<Message, ServiceError> {
        // 1. Load or create conversation
        // 2. Build prompt with context
        // 3. Call inference engine
        // 4. Save and return response
    }
}
```

#### Ports (Trait Definitions)

| Port | Description |
|------|-------------|
| `InferencePort` | LLM inference operations |
| `ConversationRepository` | Conversation persistence |
| `SecretStore` | Secret management |
| `CachePort` | Caching abstraction |
| `CalendarPort` | Calendar operations |
| `EmailPort` | Email operations |
| `WeatherPort` | Weather data |
| `SpeechPort` | STT/TTS operations |
| `WhatsAppPort` | WhatsApp messaging |
| `ApprovalRepository` | Approval persistence |
| `AuditRepository` | Audit logging |

```rust
// Example: InferencePort
#[async_trait]
pub trait InferencePort: Send + Sync {
    async fn generate(
        &self,
        prompt: &str,
        options: InferenceOptions,
    ) -> Result<InferenceResponse, InferenceError>;
    
    async fn generate_stream(
        &self,
        prompt: &str,
        options: InferenceOptions,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, InferenceError>> + Send>>, InferenceError>;
    
    async fn health_check(&self) -> Result<bool, InferenceError>;
    
    fn default_model(&self) -> &str;
}
```

---

## Infrastructure Layer

### infrastructure

**Purpose**: Provides concrete implementations of application ports for external systems.

**Dependencies**: `domain`, `application`

#### Adapters

| Adapter | Implements | Description |
|---------|------------|-------------|
| `VaultSecretStore` | `SecretStore` | HashiCorp Vault KV v2 |
| `EnvironmentSecretStore` | `SecretStore` | Environment variables |
| `ChainedSecretStore` | `SecretStore` | Multi-backend fallback |
| `Argon2PasswordHasher` | `PasswordHasher` | Secure password hashing |

```rust
// Example: VaultSecretStore usage
let vault = VaultSecretStore::new(VaultConfig {
    address: "http://127.0.0.1:8200".to_string(),
    role_id: Some("...".to_string()),
    secret_id: Some("...".to_string()),
    mount_path: "secret".to_string(),
    ..Default::default()
})?;

let secret = vault.get_secret("pisovereign/whatsapp/access_token").await?;
```

#### Cache

| Component | Description |
|-----------|-------------|
| `MokaCache` | L1 in-memory cache (fast, volatile) |
| `RedbCache` | L2 persistent cache (survives restarts) |
| `TieredCache` | Combines L1 + L2 with fallback |

```rust
// TieredCache usage
let cache = TieredCache::new(
    MokaCache::new(10_000),  // 10k entries max
    RedbCache::new("/var/lib/pisovereign/cache.redb")?,
);

// Write-through to both layers
cache.set("key", "value", Duration::from_secs(3600)).await?;

// Read checks L1 first, then L2
let value = cache.get("key").await?;
```

#### Persistence

| Component | Description |
|-----------|-------------|
| `SqliteConversationRepository` | Conversation storage |
| `SqliteApprovalRepository` | Approval request storage |
| `SqliteAuditRepository` | Audit log storage |
| `SqliteUserRepository` | User profile storage |

#### Other Components

| Component | Description |
|-----------|-------------|
| `TelemetrySetup` | OpenTelemetry initialization |
| `CronScheduler` | Cron-based task scheduling |
| `TeraTemplates` | Template rendering |
| `RetryExecutor` | Exponential backoff retry |
| `SecurityValidator` | Config validation |

---

## AI Crates

### ai_core

**Purpose**: Inference engine abstraction and Hailo-Ollama client.

**Dependencies**: `domain`, `application`

#### Components

| Component | Description |
|-----------|-------------|
| `HailoClient` | Hailo-Ollama HTTP client |
| `ModelSelector` | Dynamic model routing |

```rust
// HailoClient usage
let client = HailoClient::new(InferenceConfig {
    base_url: "http://localhost:11434".to_string(),
    default_model: "qwen2.5-1.5b-instruct".to_string(),
    timeout_ms: 60000,
    ..Default::default()
})?;

let response = client.generate(
    "What is the capital of France?",
    InferenceOptions::default(),
).await?;
```

```rust
// ModelSelector for complexity-based routing
let selector = ModelSelector::new(ModelSelectorConfig {
    small_model: "qwen2.5-1.5b-instruct".to_string(),
    large_model: "qwen2.5-7b-instruct".to_string(),
    complexity_word_threshold: 100,
    complexity_keywords: vec!["analyze", "explain", "compare"],
});

let model = selector.select_model(&prompt);
```

### ai_speech

**Purpose**: Speech-to-Text and Text-to-Speech processing.

**Dependencies**: `domain`, `application`

#### Providers

| Provider | Description |
|----------|-------------|
| `HybridSpeechProvider` | Local first, cloud fallback |
| `LocalSttProvider` | whisper.cpp integration |
| `LocalTtsProvider` | Piper integration |
| `OpenAiSpeechProvider` | OpenAI Whisper & TTS |

```rust
// HybridSpeechProvider usage
let speech = HybridSpeechProvider::new(SpeechConfig {
    provider: SpeechProviderType::Hybrid,
    prefer_local: true,
    allow_cloud_fallback: true,
    ..Default::default()
})?;

// Transcribe audio
let text = speech.transcribe(&audio_data, "en").await?;

// Synthesize speech
let audio = speech.synthesize("Hello, world!", "en").await?;
```

#### Audio Conversion

| Component | Description |
|-----------|-------------|
| `AudioConverter` | FFmpeg-based format conversion |

Supported formats: OGG/Opus, MP3, WAV, FLAC, M4A, WebM

---

## Integration Crates

### integration_whatsapp

**Purpose**: WhatsApp Business API integration.

**Dependencies**: `domain`, `application`

#### Components

| Component | Description |
|-----------|-------------|
| `WhatsAppClient` | Meta Graph API client |
| `WebhookHandler` | Incoming message handler |
| `SignatureValidator` | HMAC-SHA256 verification |

```rust
// WhatsAppClient usage
let whatsapp = WhatsAppClient::new(WhatsAppConfig {
    access_token: "...".to_string(),
    phone_number_id: "...".to_string(),
    api_version: "v18.0".to_string(),
})?;

// Send text message
whatsapp.send_text("+1234567890", "Hello!").await?;

// Send audio message
whatsapp.send_audio("+1234567890", &audio_data).await?;
```

### integration_proton

**Purpose**: Proton Mail Bridge integration via IMAP/SMTP.

**Dependencies**: `domain`, `application`

#### Components

| Component | Description |
|-----------|-------------|
| `ImapClient` | Email reading |
| `SmtpClient` | Email sending |
| `ProtonMailAdapter` | Combined email operations |

```rust
// ProtonMailAdapter usage
let mail = ProtonMailAdapter::new(ProtonConfig {
    imap_host: "127.0.0.1".to_string(),
    imap_port: 1143,
    smtp_host: "127.0.0.1".to_string(),
    smtp_port: 1025,
    email: "user@proton.me".to_string(),
    password: "bridge-password".to_string(),
})?;

// Fetch recent emails
let emails = mail.fetch_recent(10).await?;

// Send email
mail.send(EmailMessage {
    to: "recipient@example.com".to_string(),
    subject: "Hello".to_string(),
    body: "Message body".to_string(),
}).await?;
```

### integration_caldav

**Purpose**: CalDAV calendar integration.

**Dependencies**: `domain`, `application`

#### Components

| Component | Description |
|-----------|-------------|
| `CalDavClient` | CalDAV protocol client |
| `ICalParser` | iCalendar parsing |

```rust
// CalDavClient usage
let calendar = CalDavClient::new(CalDavConfig {
    server_url: "https://cal.example.com/dav.php".to_string(),
    username: "user".to_string(),
    password: "pass".to_string(),
    calendar_path: "/calendars/user/default/".to_string(),
})?;

// Fetch events
let events = calendar.get_events(start_date, end_date).await?;

// Create event
calendar.create_event(CalendarEvent {
    title: "Meeting".to_string(),
    start: start_time,
    end: end_time,
    ..Default::default()
}).await?;
```

### integration_weather

**Purpose**: Open-Meteo weather API integration.

**Dependencies**: `domain`, `application`

#### Components

| Component | Description |
|-----------|-------------|
| `OpenMeteoClient` | Weather API client |

```rust
// OpenMeteoClient usage
let weather = OpenMeteoClient::new(WeatherConfig {
    base_url: "https://api.open-meteo.com/v1".to_string(),
    forecast_days: 7,
    cache_ttl_minutes: 30,
})?;

// Get current weather
let current = weather.get_current(52.52, 13.405).await?;

// Get forecast
let forecast = weather.get_forecast(52.52, 13.405).await?;
```

---

## Presentation Crates

### presentation_http

**Purpose**: HTTP REST API using Axum.

**Dependencies**: All crates (orchestration layer)

#### Handlers

| Handler | Endpoint | Description |
|---------|----------|-------------|
| `health` | `GET /health` | Liveness probe |
| `ready` | `GET /ready` | Readiness with inference status |
| `chat` | `POST /v1/chat` | Send chat message |
| `chat_stream` | `POST /v1/chat/stream` | Streaming chat (SSE) |
| `commands` | `POST /v1/commands` | Execute command |
| `webhooks` | `POST /v1/webhooks/whatsapp` | WhatsApp webhook |
| `metrics` | `GET /metrics/prometheus` | Prometheus metrics |

#### Middleware

| Middleware | Description |
|------------|-------------|
| `RateLimiter` | Request rate limiting |
| `ApiKeyAuth` | API key authentication |
| `RequestId` | Request correlation ID |
| `Cors` | CORS handling |

#### Binaries

- `pisovereign-server` - HTTP server binary

### presentation_cli

**Purpose**: Command-line interface using Clap.

**Dependencies**: Core crates

#### Commands

| Command | Description |
|---------|-------------|
| `status` | Show system status |
| `chat` | Send chat message |
| `command` | Execute command |
| `backup` | Database backup |
| `restore` | Database restore |
| `migrate` | Run migrations |
| `openapi` | Export OpenAPI spec |

```bash
# Examples
pisovereign-cli status
pisovereign-cli chat "Hello"
pisovereign-cli command "briefing"
pisovereign-cli backup --output backup.db
pisovereign-cli openapi --output openapi.json
```

#### Binaries

- `pisovereign-cli` - CLI binary

---

## Cargo Docs

For detailed API documentation, see the auto-generated Cargo docs:

- **Latest**: [/api/latest/](../api/latest/presentation_http/index.html)
- **By Version**: `/api/vX.Y.Z/`

Generate locally:

```bash
just docs
# Opens browser at target/doc/presentation_http/index.html
```
