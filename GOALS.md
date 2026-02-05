# Goals

Build a locally operated, secure, and extensible AI assistant platform on a Raspberry Pi 5 (8 GB RAM) with Hailo-10H AI HAT+ 2.

## Focus

- Rust backend as core
- Local inference of lightweight, quantized AI models (e.g., Qwen3 4B, Phi-4 Mini, EXAONE 1.2B)
- EU/GDPR-compliant operation with European technologies (Baïkal/Radicale, Hetzner/OVH/Scaleway, Proton Mail, Matrix/Signal/Threema)
- Full control via WhatsApp messages: I send a WhatsApp message to my Raspberry Pi bot and it automatically executes my request (with sensible security approvals).

## Non-Goals

- No vendor lock-in to US cloud models in the default path.
- No unprotected remote access without VPN/tunnel.
- No monolithic codebase; modular, clearly separated components should emerge.

---

## High-Level Functionality (Clowdbot-inspired, but local/European)

### AI-Powered Task Automation
- Automatic responses and drafts for user requests
- Email management (e.g., classification, summaries, response suggestions)
- Schedule planning, reminders, task prioritization

### External Service Integration
- Email via Proton Mail (Bridge / unofficial APIs in separate sidecar)
- Messaging via WhatsApp Cloud/Business API and optionally EU-friendly alternatives like Matrix/Element, Signal, Threema
- Calendar via CalDAV (Baïkal, Radicale, optionally DAViCal/Nextcloud)
- Optional cloud storage or additional APIs via plugins

### User Interfaces
- Web interface (e.g., Solid.js/Svelte/React)
- CLI for configuration/administration
- Optional voice assistant (Rhasspy/Mycroft) for local voice input/output

### Security Features
- Encryption, authentication, data isolation
- Sandboxing of AI processes and integration components
- Audit logs, approval gates for critical actions

### Scalability & Performance
- Optimized for Raspberry Pi 5 + Hailo-10H
- Edge AI optimization, quantization, efficient inference pipelines
- Modular, easily portable to e.g., NVIDIA Jetson or Hailo Dongle

---

## Architecture and Code Principles (Rust, Typing, Small Files, Clean Architecture)

### Architecture Style
- Ports-and-Adapters / Hexagonal / Clean Architecture
- Clear layers:
  - **domain**: Use-Cases, Entities, Value Objects, Domain-Services
  - **application**: Orchestration, Commands/Queries, Agent logic
  - **infrastructure**: Adapters to DB, HTTP, AI, WhatsApp, Proton, CalDAV, Storage
  - **presentation**: HTTP-API, CLI, optionally WebSocket/SSE endpoints

### Rust-Specific Requirements

#### Strong Typing
- Use specific types (`EmailAddress`, `PhoneNumber`, `UserId`, `ConversationId`, `ApiToken`, `CalendarEventId`) instead of naked strings/ints.
- Model states as enums (`WhatsAppSessionState`, `TaskStatus`, `AgentCommand`, `AgentActionOutcome`).
- Errors via `Result<T, DomainError>` / specific error types per layer.
- No "magic strings" for commands; use typed Command structures.

#### Project Structure
- Rust workspace with crates like:
  - `domain`
  - `application`
  - `infrastructure`
  - `ai_core`
  - `presentation_http`
  - `presentation_cli`
  - `integration_whatsapp`
  - `integration_proton`
  - `integration_caldav`
- Keep each file small (guideline < 300 lines) and with a single clear responsibility (SRP).

#### Async & Concurrency
- Tokio as runtime, no blocking operations in async context.
- Use channels (mpsc) and clear task boundaries.
- Rate limits & circuit breakers for external services (WhatsApp/Proton/CalDAV).

#### FFI & AI Bindings
- Inference frameworks: ONNX Runtime, TensorFlow Lite, Llama.cpp/GGML; optionally Hailo SDK.
- Encapsulate FFI interactions via safe wrapper classes; raw pointers remain in internal module.

#### Tests
- Unit tests for domain logic (parsing, classification, task state machines).
- Integration tests for HTTP-API, WhatsApp webhooks, CalDAV, Proton sidecar.
- Property tests for parser/mapper that convert text → Commands/Entities.

---

## AI and Inference Layer

### Hardware
- Raspberry Pi 5 with 8 GB RAM + Hailo-10H AI HAT+ 2
- Goal: Response times in the range of approx. 100–500 ms per request (depending on model/complexity).

### Models
- **Qwen3 4B** (Edge-capable, 4B parameters, 256K context, strong in logic/code)
- **Phi-4 Mini** (~3.8B, fast responses, good security focus)
- **EXAONE 1.2B** (1.2B, multilingual, efficient for agent functions)

### Pipeline
- **Model Manager**:
  - Loading, versioning, model selection (routing).
  - Management of quantized models (INT4/INT8).
- **Inference Engine**:
  - Streaming token output (for chat/response streaming).
  - Configurable sampling parameters.
  - Metrics (tokens/s, latency, RAM usage).

### Fine-Tuning / Customization
- Optional Low-Rank Adaptation (LoRA) or Hailo Dataflow Compiler, e.g., for:
  - Email classification
  - Command recognition from WhatsApp messages
  - Personal writing styles/response templates

---

## Integrations (Email, Calendar, WhatsApp)

### 1. Email (Proton Mail)

#### Architecture
- Separate sidecar service using Proton Mail Bridge or unofficial APIs.
- Communication with core service via a defined HTTP/gRPC or message-based interface.

#### Capabilities
- Polling new emails for defined accounts.
- Classification (important, newsletter, ToDo, private).
- Generation of response drafts and summaries.

#### Security
- Tokens/passwords never in code; exclusively in Secret-Store/Env.
- Sidecar runs under its own, restricted user.

### 2. Calendar (Baïkal, Radicale, DAViCal, Nextcloud)

- Focus on lightweight, Pi-friendly servers (Baïkal, Radicale).
- CalDAV client in core service:
  - Sync worker for pull/push of events.
  - Support for deltas instead of full sync.

#### Features
- Create, modify, and delete events.
- "Morning Briefing": daily overview of relevant appointments.

#### Typing
- Custom types for `CalendarEvent`, `CalendarId`, `Attendee`, `TimeWindow`.

### 3. WhatsApp as Central Control and Interaction Interface

#### Goal
- I send a WhatsApp message to the bot number → the bot interprets the message, maps it to a typed `AgentCommand`, and executes the desired action (email, calendar, internal queries, etc.), with security approval where necessary.

#### Architecture
- WhatsApp Gateway component (Rust or Node.js), ideally its own module/service:
  - Receives webhook requests from WhatsApp Cloud/Business API.
  - Validates signatures/payload.
  - Extracts text, sender, metadata.
  - Maps text → internal `WhatsAppMessageInput` → `application::handle_incoming_whatsapp_message()`.
  - Sends responses/status back via WhatsApp API.

#### Commands
- Define `AgentCommand` as a strongly typed enum, e.g.:
  - `MorningBriefing { date: Date }`
  - `CreateCalendarEvent { date, time, title, attendees? }`
  - `SummarizeInbox`
  - `DraftEmail { to: EmailAddress, subject?, body }`
  - `SystemCommand { reboot?, status?, version? }`

#### Parsing Pipeline
1. LLM receives raw text and extracts intent + parameters (slots).
2. Application layer validates and builds an `AgentCommand` from it.
3. `AgentCommand` is routed to the appropriate use case.

#### Security Mechanisms
- Whitelist of trusted phone numbers.
- Optional "/unlock <PIN>" mechanism per session for critical actions (send email, reschedule appointments, control devices).
- Rate limits per sender and global.
- Audit log of all actions triggered via WhatsApp (time, number, command, result).

---

## Security, Privacy, EU Compliance

### Cryptography
- TLS for all HTTP connections (internal & external).
- Encryption of sensitive data "at rest" (e.g., encrypted DB or encrypted columns/secrets).
- AES-based encryption for API tokens, refresh keys, etc.

### Authentication & Authorization
- Web UI secured by OAuth2/OIDC (optionally Keycloak / local IdP) or strong local user accounts.
- Role-Based Access Control: admin, user, service.
- API keys/tokens for plugins and integrations.

### Process and System Hardening
- Separate Unix users for Core Service, WhatsApp Gateway, Proton Sidecar, optionally CalDAV Client.
- Use of Systemd sandboxing (PrivateTmp, ProtectHome, RestrictAddressFamilies, NoNewPrivileges).
- Optional containerization of risky components.

### Agent Guardrails
- Approval gates for all actions with external effect:
  - Email sending: show draft for confirmation first (e.g., via WhatsApp or Web UI).
  - Calendar changes: show summary, then wait for "OK".
- Policy engine that defines:
  - Which tools/plugins the agent may use automatically.
  - Which may only be used with approval.

### Content Boundaries
- No sharing of sensitive data with external services without marking.

### EU/GDPR Focus
- Local processing of all content by default.
- Optional remote fallback to large models only after explicit approval.
- Data minimization, clear deletion concepts, exportable user data.

---

## Extensibility and Plugin System

### Plugins/Skills
- Plugins as processes with defined protocol (e.g., JSON-RPC via stdin/stdout or HTTP).
- Each plugin describes:
  - Name, version, permissions (calendar, email, files, network).
  - Input/output schema.
- Examples:
  - Weather plugin (e.g., EU API).
  - News summarization.
  - IoT/MQTT control.

### Skill Concept
- Skill = combination of:
  - Prompt template (for LLM),
  - Available tools/plugins,
  - Policies (auto-run vs. require-approval).

---

## Roadmap (for Agent Mode)

### Phase 1 – Core & AI (approx. 80–120 h)
- Set up Rust workspace according to Clean Architecture structure.
- Define Domain & Application layer (`AgentCommand`, Use Cases).
- Implement HTTP-API (e.g., `/v1/chat`, `/v1/commands`).
- Integrate first local quantized model (Qwen3 4B / Phi-4 Mini).

### Phase 2 – Integrations (approx. 80–120 h)
- CalDAV client + sync worker (Baïkal/Radicale).
- Proton Mail sidecar with defined, stable interface.
- WhatsApp gateway with webhook endpoint and command parsing.
- First WhatsApp commands: ping, help, echo, status.

### Phase 3 – Agent Features & UI (approx. 60–100 h)
- Morning briefing, inbox summary, simple task automation via WhatsApp.
- Web UI + CLI for configuration, logs, health checks.
- Plugin/skill system with 1-2 example plugins.

### Phase 4 – Security & Hardening (approx. 40–80 h)
- Auth, role model, secrets handling, TLS.
- Systemd hardening, logging/monitoring/metrics.
- Performance tuning on Raspberry Pi 5 + Hailo-10H, load tests.

---

## Workflow Guidelines (for Copilot/Claude Agent)

- Write idiomatic, strongly typed, well-tested Rust code.
- Prefer:
  - Small, focused modules and files.
  - Clear interfaces/traits for ports, adapters in separate modules.
  - Simple, readable solutions instead of overly complex generics/abstractions.
- Approach for new features:
  1. Briefly sketch the architecture impact (Markdown comment).
  2. Define interfaces/traits for Domain/Application/Ports.
  3. Implement minimal end-to-end flow (e.g., WhatsApp message → AgentCommand → Use Case → Response).
  4. Add tests (unit + optionally integration).
