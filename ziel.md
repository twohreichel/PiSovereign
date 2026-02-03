Ziel:
Baue eine lokal betriebene, sichere und erweiterbare KI-Assistenz-Plattform auf einem Raspberry Pi 5 (8 GB RAM) mit Hailo-10H AI HAT+ 2. 
Fokus:
- Rust-Backend als Kern
- Lokale Inferenz leichter, quantisierter KI-Modelle (z.B. Qwen3 4B, Phi-4 Mini, EXAONE 1.2B)
- EU-/DSGVO-konformer Betrieb mit europäischen Technologien (Baïkal/Radicale, Hetzner/OVH/Scaleway, Proton Mail, Matrix/Signal/Threema)
- Vollständige Steuerung u.a. per WhatsApp-Nachrichten: Ich schicke eine WhatsApp-Nachricht an meinen Raspberry-Pi-Bot und er führt meine Anfrage automatisch aus (mit sinnvollen Sicherheits-Freigaben).

Nicht-Ziele:
- Kein Vendor-Lock-in an US-Cloud-Modelle im Standardpfad.
- Kein ungeschützter Remote-Zugriff ohne VPN/Tunnel.
- Keine monolithische Codebasis; es sollen modulare, klar getrennte Komponenten entstehen.

---

High-Level-Funktionalität (Clowdbot-inspiriert, aber lokal/europäisch):

- KI-gestützte Aufgabenautomatisierung:
  - Automatische Antworten und Entwürfe für Benutzeranfragen
  - E-Mail-Verwaltung (z.B. Klassifikation, Zusammenfassungen, Antwortvorschläge)
  - Terminplanung, Erinnerungen, Priorisierung von Tasks

- Integration externer Dienste:
  - E-Mail über Proton Mail (Bridge / inoffizielle APIs in separatem Sidecar)
  - Messaging über WhatsApp Cloud/Business API und optional EU-freundliche Alternativen wie Matrix/Element, Signal, Threema
  - Kalender über CalDAV (Baïkal, Radicale, optional DAViCal/Nextcloud)
  - Optional Cloud-Speicher oder weitere APIs über Plugins

- Benutzeroberflächen:
  - Web-Interface (z.B. Solid.js/Svelte/React)
  - CLI für Konfiguration/Administration
  - Optional Voice-Assistent (Rhasspy/Mycroft) für lokale Spracheingabe/-ausgabe

- Sicherheitsfeatures:
  - Verschlüsselung, Authentifizierung, Datenisolierung
  - Sandboxing der KI-Prozesse und Integrationskomponenten
  - Audit-Logs, Approval-Gates für kritische Aktionen

- Skalierbarkeit & Performance:
  - Optimiert für Raspberry Pi 5 + Hailo-10H
  - Edge-KI-Optimierung, Quantisierung, effiziente Inferenzpipelines
  - Modular, leicht portierbar auf z.B. NVIDIA Jetson oder Hailo-Dongle

---

Architektur- und Code-Prinzipien (Rust, Typisierung, kleine Dateien, Clean Architecture):

- Architektur-Stil:
  - Ports-and-Adapters / Hexagonal / Clean Architecture
  - Klare Schichten:
    - domain: Use-Cases, Entities, Value Objects, Domain-Services
    - application: Orchestrierung, Commands/Queries, Agent-Logik
    - infrastructure: Adapter zu DB, HTTP, KI, WhatsApp, Proton, CalDAV, Storage
    - presentation: HTTP-API, CLI, ggf. WebSocket/SSE-Endpunkte

- Rust-spezifische Anforderungen:
  - Starke Typisierung:
    - Verwende spezifische Typen (EmailAddress, PhoneNumber, UserId, ConversationId, ApiToken, CalendarEventId) statt nackter Strings/ints.
    - Zustände als Enums modellieren (WhatsAppSessionState, TaskStatus, AgentCommand, AgentActionOutcome).
    - Fehler über Result<T, DomainError> / spezifische Error-Typen pro Schicht.
    - Keine „magischen Strings“ für Kommandos; nutze getypte Command-Strukturen.
  - Projektstruktur:
    - Rust-Workspace mit Crates wie:
      - domain
      - application
      - infrastructure
      - ai_core
      - presentation_http
      - presentation_cli
      - integration_whatsapp
      - integration_proton
      - integration_caldav
    - Jede Datei klein halten (Richtwert < 300 Zeilen) und nur eine klare Verantwortlichkeit (SRP).
  - Async & Concurrency:
    - Tokio als Runtime, keine blockierenden Operationen im async-Kontext.
    - Nutzung von Channels (mpsc) und klaren Task-Grenzen.
    - Rate-Limits & Circuit-Breaker für externe Dienste (WhatsApp/Proton/CalDAV).
  - FFI & KI-Bindings:
    - Inferenz-Frameworks: ONNX Runtime, TensorFlow Lite, Llama.cpp/GGML; optional Hailo SDK.
    - FFI-Interaktionen über sichere Wrapper-Klassen kapseln; Rohzeiger bleiben in internem Modul.
  - Tests:
    - Unit-Tests für Domain-Logik (Parsing, Klassifikation, Task-State-Maschinen).
    - Integrationstests für HTTP-API, WhatsApp-Webhooks, CalDAV, Proton-Sidecar.
    - Property-Tests für Parser/Mapper, die Text → Commands/Entities umsetzen.

---

KI- und Inferenz-Schicht:

- Hardware:
  - Raspberry Pi 5 mit 8 GB RAM + Hailo-10H AI HAT+ 2
  - Ziel: Antwortzeiten im Bereich von ca. 100–500 ms pro Anfrage (je nach Modell/Komplexität).

- Modelle:
  - Qwen3 4B (Edge-tauglich, 4B Parameter, 256K Kontext, stark in Logik/Code)
  - Phi-4 Mini (~3.8B, schnelle Antworten, guter Sicherheitsfokus)
  - EXAONE 1.2B (1.2B, mehrsprachig, effizient für Agentenfunktionen)

- Pipeline:
  - Model Manager:
    - Laden, Versionierung, Auswahl (Routing) des Modells.
    - Verwaltung quantisierter Modelle (INT4/INT8).
  - Inference Engine:
    - Streaming-Token-Ausgabe (für Chat/Antwort-Streaming).
    - Konfigurierbare Sampling-Parameter.
    - Metriken (Token/s, Latenz, RAM-Verbrauch).

- Fine-Tuning / Anpassung:
  - Optional Low-Rank Adaptation (LoRA) oder Hailo Dataflow Compiler, z.B. für:
    - E-Mail-Klassifizierung
    - Befehlserkennung aus WhatsApp-Nachrichten
    - Persönliche Schreibstile/Antwortvorlagen

---

Integrationen (E-Mail, Kalender, WhatsApp):

1. E-Mail (Proton Mail):
   - Architektur:
     - Separater Sidecar-Dienst, der Proton Mail Bridge oder inoffizielle APIs nutzt.
     - Kommunikation mit Core-Service über eine definierte HTTP/gRPC- oder Message-basierte Schnittstelle.
   - Fähigkeiten:
     - Polling neuer E-Mails für definierte Accounts.
     - Klassifikation (wichtig, Newsletter, ToDo, privat).
     - Generierung von Antwortentwürfen und Zusammenfassungen.
   - Sicherheit:
     - Tokens/Passwörter nie im Code; ausschließlich in Secret-Store/Env.
     - Sidecar läuft unter eigenem, eingeschränktem User.

2. Kalender (Baïkal, Radicale, DAViCal, Nextcloud):
   - Fokus auf leichtgewichtige, Pi-freundliche Server (Baïkal, Radicale).
   - CalDAV-Client im Core-Service:
     - Sync-Worker für Pull/Push von Events.
     - Unterstützung von Deltas statt Full-Sync.
   - Features:
     - Erstellen, Anpassen und Löschen von Events.
     - „Morning Briefing“: tägliche Übersicht relevanter Termine.
   - Typisierung:
     - Eigene Typen für CalendarEvent, CalendarId, Attendee, TimeWindow.

3. WhatsApp als zentrale Steuer- und Interaktionsschnittstelle:
   - Ziel:
     - Ich schicke eine WhatsApp-Nachricht an die Bot-Nummer → der Bot interpretiert die Nachricht, mappt sie auf einen getypten AgentCommand und führt die gewünschte Aktion aus (E-Mail, Kalender, interne Queries etc.), mit Sicherheits-Freigabe, wo nötig.
   - Architektur:
     - WhatsApp-Gateway-Komponente (Rust oder Node.js), idealerweise eigenes Modul/Service:
       - Empfängt Webhook-Requests der WhatsApp Cloud/Business API.
       - Validiert Signaturen/Payload.
       - Extrahiert Text, Absender, Meta-Daten.
       - Mapped Text → internes WhatsAppMessageInput → application::handle_incoming_whatsapp_message().
       - Versendet Antworten/Status über WhatsApp API zurück.
   - Kommandos:
     - Definiere AgentCommand als stark typisierte Enum, z.B.:
       - MorningBriefing { date: Date }
       - CreateCalendarEvent { date, time, title, attendees? }
       - SummarizeInbox
       - DraftEmail { to: EmailAddress, subject?, body }
       - SystemCommand { reboot?, status?, version? }
     - Parsing-Pipeline:
       1. LLM erhält Rohtext und extrahiert Intent + Parameter (Slots).
       2. Application-Schicht validiert und baut daraus einen AgentCommand.
       3. AgentCommand wird dem passenden Use-Case zugeführt.
   - Sicherheitsmechanismen:
     - Whitelist vertrauenswürdiger Telefonnummern.
     - Optionaler „/unlock <PIN>“-Mechanismus pro Session für kritische Aktionen (E-Mail senden, Termine verschieben, Geräte steuern).
     - Rate-Limits pro Absender und global.
     - Audit-Log aller per WhatsApp ausgelösten Aktionen (Zeit, Nummer, Command, Ergebnis).

---

Sicherheit, Datenschutz, EU-Konformität:

- Kryptografie:
  - TLS für alle HTTP-Verbindungen (intern & extern).
  - Verschlüsselung sensibler Daten „at rest“ (z.B. verschlüsselte DB oder verschlüsselte Spalten/Secrets).
  - AES-basierte Verschlüsselung für API-Tokens, Refresh-Keys etc.

- Authentifizierung & Autorisierung:
  - Web-UI abgesichert durch OAuth2/OIDC (ggf. Keycloak / lokaler IdP) oder starke lokale Nutzerkonten.
  - Role-Based Access Control: admin, user, service.
  - API-Keys/Tokens für Plugins und Integrationen.

- Prozess- und System-Hardening:
  - Getrennte Unix-User für Core-Service, WhatsApp-Gateway, Proton-Sidecar, ggf. CalDAV-Client.
  - Nutzung von Systemd-Sandboxing (PrivateTmp, ProtectHome, RestrictAddressFamilies, NoNewPrivileges).
  - Optionale Containerisierung riskanter Komponenten.

- Agent-Guardrails:
  - Approval-Gates für alle Aktionen mit externem Effekt:
    - E-Mail-Versand: Entwurf zuerst zur Bestätigung anzeigen (z.B. per WhatsApp oder Web-UI).
    - Kalenderänderungen: Zusammenfassung zeigen, dann „OK“ abwarten.
  - Policy-Engine, die definiert:
    - Welche Tools/Plugins der Agent automatisch nutzen darf.
    - Welche nur mit Freigabe verwendet werden dürfen.
  - Content-Grenzen:
    - Keine Weitergabe sensibler Daten an externe Dienste ohne Kennzeichnung.

- EU-/DSGVO-Fokus:
  - Standardmäßig lokale Verarbeitung aller Inhalte.
  - Optionaler Remote-Fallback zu großen Modellen nur nach expliziter Freigabe.
  - Datenminimierung, klare Löschkonzepte, exportierbare Nutzer-Daten.

---

Erweiterbarkeit und Plugin-System:

- Plugins/Skills:
  - Plugins als Prozesse mit definiertem Protokoll (z.B. JSON-RPC über stdin/stdout oder HTTP).
  - Jedes Plugin beschreibt:
    - Name, Version, Berechtigungen (Kalender, E-Mail, Dateien, Netzwerk).
    - Input-/Output-Schema.
  - Beispiele:
    - Wetter-Plugin (z.B. EU-API).
    - Nachrichten-Zusammenfassung.
    - IoT/MQTT-Steuerung.

- Skill-Konzept:
  - Skill = Kombination aus:
    - Prompt-Schablone (für LLM),
    - verfügbaren Tools/Plugins,
    - Policies (auto-run vs. require-approval).

---

Roadmap (für den Agent-Mode):

Phase 1 – Core & KI (ca. 80–120 h):
- Rust-Workspace nach Clean-Architecture-Struktur aufsetzen.
- Domain & Application-Schicht definieren (AgentCommand, Use-Cases).
- HTTP-API (z.B. /v1/chat, /v1/commands) implementieren.
- Erstes lokales quantisiertes Modell (Qwen3 4B / Phi-4 Mini) integrieren.

Phase 2 – Integrationen (ca. 80–120 h):
- CalDAV-Client + Sync-Worker (Baïkal/Radicale).
- Proton-Mail-Sidecar mit definierter, stabiler Schnittstelle.
- WhatsApp-Gateway mit Webhook-Endpoint und Command-Parsing.
- Erste WhatsApp-Commands: ping, hilfe, echo, status.

Phase 3 – Agent-Features & UI (ca. 60–100 h):
- Morning-Briefing, Inbox-Summary, einfache Task-Automatisierung über WhatsApp.
- Web-UI + CLI für Konfiguration, Logs, Health-Checks.
- Plugin-/Skill-System mit 1–2 Beispiel-Plugins.

Phase 4 – Security & Hardening (ca. 40–80 h):
- Auth, Rollenmodell, Secrets-Handling, TLS.
- Systemd-Hardening, Logging/Monitoring/Metriken.
- Performance-Tuning auf Raspberry Pi 5 + Hailo-10H, Lasttests.

---

Arbeitsweise für dich (Copilot/Claude Agent):

- Schreibe idiomatischen, stark typisierten, gut getesteten Rust-Code.
- Bevorzuge:
  - Kleine, fokussierte Module und Dateien.
  - Klare Interfaces/Traits für Ports, Adapter in separaten Modulen.
  - Einfache, lesbare Lösungen statt überkomplexer Generics/Abstraktionen.
- Vorgehen bei neuen Features:
  1. Kurz die Architektur-Impact skizzieren (Markdown-Kommentar).
  2. Interfaces/Traits für Domain/Application/Ports definieren.
  3. Minimalen End-to-End-Flow umsetzen (z.B. WhatsApp-Nachricht → AgentCommand → Use-Case → Antwort).
  4. Tests hinzufügen (Unit + ggf. Integration).
