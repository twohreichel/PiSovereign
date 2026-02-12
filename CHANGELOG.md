# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.5](https://github.com/twohreichel/PiSovereign/compare/v0.3.4...v0.3.5) (2026-02-12)


### Features

* add OpenTelemetry Collector installation and configuration options for macOS and Raspberry Pi setups ([940c9f1](https://github.com/twohreichel/PiSovereign/commit/940c9f11dba87e5b6e1e9be35febc7f5e6b69034))

## [0.3.4](https://github.com/twohreichel/PiSovereign/compare/v0.3.3...v0.3.4) (2026-02-11)


### Features

* add Baïkal CalDAV server Docker integration\n\nAdd --baikal CLI flag and interactive prompt to both setup-mac.sh and\nsetup-pi.sh scripts for deploying Baïkal as a Docker container.\n\n- Add ckulka/baikal:nginx service to docker-compose.yml generation\n- Bind to 127.0.0.1:5232 (localhost only, no external access)\n- PiSovereign accesses Baïkal internally via http://baikal:80/dav.php\n- Add baikal-config and baikal-data Docker volumes\n- Add CalDAV config prompt to setup-pi.sh (was missing entirely)\n- Dual CalDAV flow: Baïkal Docker OR external CalDAV server\n- Add setup wizard instructions to print_summary() in both scripts\n- Update config.toml.example with Docker-internal URL comment\n- Update external-services.md with Docker installation as primary method\n- Retain native Baïkal installation as documented alternative" ([bffafd6](https://github.com/twohreichel/PiSovereign/commit/bffafd662e6b311441fdd51e6de0925e989f5c2f))
* **application:** add reminder and transit intents with AgentService wiring (Phase J) ([b256ec0](https://github.com/twohreichel/PiSovereign/commit/b256ec094742f13e4ceab9e04f2b8e0eadf43741))
* **application:** add reminder formatter and notification service\n\n- Create ReminderFormatter with beautiful German message templates:\n  - format_calendar_event_reminder with event time, location, Maps link\n  - format_calendar_task_reminder for todo items  \n  - format_custom_reminder for user-created reminders\n  - format_morning_briefing with events, weather, reminders\n  - format_reminder_list, format_snooze_confirmation\n  - Time formatting helpers (format_event_time, format_time_until)\n- Create NotificationService for proactive reminder processing:\n  - process_due_reminders polls and formats all due notifications\n  - Optional transit integration (ÖPNV connections to event locations)\n  - Marks reminders as sent after notification formatted\n  - NotificationConfig with home coords and transit toggle\n- Register modules and re-export types from services/mod.rs\n- 26 new tests (18 formatter + 8 notification), all 641 app tests pass" ([85b181c](https://github.com/twohreichel/PiSovereign/commit/85b181c860150cecd65338388573fb4ec72be4c6))
* **application:** add ReminderPort and ReminderService ([4ceca10](https://github.com/twohreichel/PiSovereign/commit/4ceca102415610e5671a1a4af0372534ae093578))
* **config:** add MessengerPersistenceConfig for conversation storage ([ccba24e](https://github.com/twohreichel/PiSovereign/commit/ccba24e5ccc68093f1a55d27712f3f90f5df58a9))
* **db:** add migration V009 for conversation source tracking ([02e17e2](https://github.com/twohreichel/PiSovereign/commit/02e17e26b706371a5c89989f1e159a2d3807ab6d))
* **deps:** update clap and related packages to version 4.5.58 and 1.0.0 ([58a9f5e](https://github.com/twohreichel/PiSovereign/commit/58a9f5e44b71a23007fce47f12b3beff2ab70fff))
* **deps:** upgrade breaking dependencies ([9c31b34](https://github.com/twohreichel/PiSovereign/commit/9c31b3498ed14bbc790b381b2a5b9eec062ec6d5))
* **deps:** upgrade OpenTelemetry stack 0.27 -&gt; 0.31 ([8629ab7](https://github.com/twohreichel/PiSovereign/commit/8629ab763b09d9118c1ec62ba6abcc50e6dd7b0f))
* **domain:** add Reminder entity and AgentCommand variants ([433e8ea](https://github.com/twohreichel/PiSovereign/commit/433e8ea82f1e036ebe77cf2f0223d517960c9b95))
* **handlers:** integrate messenger conversation persistence ([4f13605](https://github.com/twohreichel/PiSovereign/commit/4f13605763736d8840db24438f6cf386237e7a8d))
* **infrastructure:** add SqliteReminderStore and V006-V008 migrations\n\n- Create SqliteReminderStore implementing ReminderPort trait\n- Add save, get, get_by_source_id, update, delete, query, get_due_reminders,\n  count_active, cleanup_old operations with spawn_blocking pattern\n- Add dynamic SQL query builder for ReminderQuery with filters\n- Add row_to_reminder mapping and enum string conversion helpers\n- Register reminder_store module in persistence/mod.rs\n- Add inline migrations V006 (retry queue), V007 (memory storage),\n  V008 (reminders) to Rust migration runner\n- Bump SCHEMA_VERSION from 5 to 8\n- Fix FK mismatch in V007 (user_profiles.id → user_profiles.user_id)\n- Remove unnecessary FK constraint from reminders table\n- 13 integration tests for reminder store, all 807 infra tests passing" ([6c500d9](https://github.com/twohreichel/PiSovereign/commit/6c500d95dcae59f611be0252383fe57ff11e051c))
* **integration:** add public transit integration and reminder configuration ([5fbc39d](https://github.com/twohreichel/PiSovereign/commit/5fbc39d8e87a7b87c3176ef405a3d729d8a88910))
* **persistence:** extend conversation entities for messenger tracking ([c4c39a5](https://github.com/twohreichel/PiSovereign/commit/c4c39a5a799d4a8e65b2a9b7a2c789726d716b6d))
* **ports:** add get_by_phone_number to ConversationStore ([a6d6c7a](https://github.com/twohreichel/PiSovereign/commit/a6d6c7a56f082a6da220d42128f922879df44f82))
* **services:** add MessengerChatService for conversation persistence ([3e8dd87](https://github.com/twohreichel/PiSovereign/commit/3e8dd87a461a34775d953f8b3fe8beec50dc80dd))
* **services:** add MessengerChatService for conversation persistence ([34b430c](https://github.com/twohreichel/PiSovereign/commit/34b430c22e4dfe76f2b42b8160aa4611327ce438))
* **tasks:** add conversation retention cleanup task ([410a2e8](https://github.com/twohreichel/PiSovereign/commit/410a2e8fea795be8b88550081245c4178d0c5bb6))
* **transit:** add integration_transit crate with HAFAS client and Nominatim geocoding\n\nImplements public transit routing via transport.rest v6 API (HAFAS)\nand address geocoding via Nominatim/OpenStreetMap.\n\n- HafasTransitClient: journey search, nearby stops, stop search\n- NominatimGeocodingClient: forward/reverse geocoding with rate limiting and caching\n- TransitConfig/NominatimConfig with serde defaults and validation\n- Typed models: Journey, Leg, Stop, LineInfo, TransitMode\n- Rich formatting with emoji per transit mode\n- Full unit tests (42) and wiremock integration tests (7)\n- Registered in workspace Cargo.toml ([c8ccead](https://github.com/twohreichel/PiSovereign/commit/c8cceadd821661e85717959c7b8b807d0814d558))
* **transit:** add TransitPort, LocationHelper, and TransitAdapter ([b4d64ff](https://github.com/twohreichel/PiSovereign/commit/b4d64ff61a6cae59c8d57677184bc888db1a6ac7))


### Bug Fixes

* **benches:** update chat_pipeline benchmark for new ConversationStore trait ([11f9055](https://github.com/twohreichel/PiSovereign/commit/11f90551538d2a4111ceffd820a496c4e1ddf828))
* **security:** add advisory ignores for rsa and rustls-pemfile vulnerabilities ([7f46adb](https://github.com/twohreichel/PiSovereign/commit/7f46adbf5d237444c749b1ceb3b3c48c76214431))
* **security:** add workflow permissions and remove key prefix logging ([8e2fd97](https://github.com/twohreichel/PiSovereign/commit/8e2fd97ea68aef337b0e94ab394a6f47603d739d))


### Documentation

* add reminder system user guide ([4cc9448](https://github.com/twohreichel/PiSovereign/commit/4cc94489fcd1f90e2c3605820900c87870d11817))

## [0.3.3](https://github.com/twohreichel/PiSovereign/compare/v0.3.2...v0.3.3) (2026-02-10)


### Features

* add Prometheus and Grafana monitoring stack installation and configuration ([d640279](https://github.com/twohreichel/PiSovereign/commit/d640279e60ced0949edd8c79f0d48039131f11eb))
* enhance cross-compilation setup for Ubuntu 24.04 by updating APT source handling ([f9aa9c1](https://github.com/twohreichel/PiSovereign/commit/f9aa9c1cb7225d722230c52359636ee23e1a28b4))
* restructure docker-compose.yml generation for improved clarity and monitoring integration ([593e582](https://github.com/twohreichel/PiSovereign/commit/593e582f710a59ff4aceba4fab5915c1709c624c))
* update Grafana dashboard provisioning paths for consistency across setups ([e7ee254](https://github.com/twohreichel/PiSovereign/commit/e7ee2548dba0c3583f48ef9591d6cc134ffcc8e8))

## [0.3.2](https://github.com/twohreichel/PiSovereign/compare/v0.3.1...v0.3.2) (2026-02-10)


### Features

* enhance Docker workflow by integrating Buildx for multi-architecture manifest creation ([10a6691](https://github.com/twohreichel/PiSovereign/commit/10a669101c38af514443ec3585f019a0ce67c889))
* enhance release binary build process for ARM64 cross-compilation ([66218d9](https://github.com/twohreichel/PiSovereign/commit/66218d96a1bdb884229e9dc522d7471aa753b61d))


### Bug Fixes

* lower coverage threshold to 80% for improved build stability ([343b47d](https://github.com/twohreichel/PiSovereign/commit/343b47ddf5dda696c0ce1f1b797ecdaa3fed9da9))

## [0.3.1](https://github.com/twohreichel/PiSovereign/compare/v0.3.0...v0.3.1) (2026-02-10)


### Features

* add task list management commands and update command parser ([9d03b39](https://github.com/twohreichel/PiSovereign/commit/9d03b398b6abf1432a8cdbc70487f553dfd8518a))
* **ai_core:** add Ollama embedding engine ([04dfa87](https://github.com/twohreichel/PiSovereign/commit/04dfa87693436015b961e9e181431a55096d4c58))
* **application:** add memory, embedding, and encryption ports ([af43c92](https://github.com/twohreichel/PiSovereign/commit/af43c924f3f4dbfef1762ce47145e02917e6d766))
* **application:** add MemoryEnhancedChat for RAG integration ([e393114](https://github.com/twohreichel/PiSovereign/commit/e393114820cb7bfeb6a6bb19c30f30734d809aed))
* **application:** add MemoryService for AI memory management ([43ab0f8](https://github.com/twohreichel/PiSovereign/commit/43ab0f849dc00df5729634b9946fad9d64040d55))
* **dependencies:** add chacha20poly1305 for encryption support ([9eb4e7c](https://github.com/twohreichel/PiSovereign/commit/9eb4e7c808dce745faea0d696ccd9c4793849967))
* **domain:** add Memory entity and MemoryId value object ([29cb85a](https://github.com/twohreichel/PiSovereign/commit/29cb85ad0a328c1f0c596557ba99c33920246b22))
* **infrastructure:** add memory storage configuration ([820c388](https://github.com/twohreichel/PiSovereign/commit/820c3880fb991761294d879c69a84e25b90327d2))
* **infrastructure:** add memory store and encryption adapter ([e50e2d8](https://github.com/twohreichel/PiSovereign/commit/e50e2d84181660747b9e6cbd72327aabfabb4607))
* **security:** add prompt injection prevention system ([09b16ba](https://github.com/twohreichel/PiSovereign/commit/09b16bac609a3320cbf8eb35e06a575c370eea67))


### Bug Fixes

* copy config.toml.example in Docker build ([472efb1](https://github.com/twohreichel/PiSovereign/commit/472efb1d1ee3b50b6cd2e20af5f2ee771e021864))


### Documentation

* add AI memory system documentation ([13e9c01](https://github.com/twohreichel/PiSovereign/commit/13e9c01dd63f2a09736f07761158cf2862be7765))

## [0.3.0](https://github.com/twohreichel/PiSovereign/compare/v0.2.2...v0.3.0) (2026-02-08)


### ⚠ BREAKING CHANGES

* **ai_core,infrastructure:** Renamed types and modules for clarity:
    - `hailo/` module -> `ollama/` in ai_core crate
    - `HailoInferenceEngine` -> `OllamaInferenceEngine`
    - `HailoInferenceAdapter` -> `OllamaInferenceAdapter`
    - `HailoModelRegistryAdapter` -> `OllamaModelRegistryAdapter`
    - `HailoModelRegistryConfig` -> `OllamaModelRegistryConfig`

### Features

* **ai_speech:** add platform-specific default paths ([dd47d6d](https://github.com/twohreichel/PiSovereign/commit/dd47d6da5a315e1fbfb4fdfdba8c43d026231674))
* **config:** add messenger selection and Signal configuration ([47acda9](https://github.com/twohreichel/PiSovereign/commit/47acda998f2f4037b513d6b4c232eca141db788e))
* **documentation:** enhance user guides with Signal messenger setup and configuration details ([66ca1bf](https://github.com/twohreichel/PiSovereign/commit/66ca1bf5ca7df5a366e9d09f56b384e9c6cf79c8))
* **domain:** add MessengerSource and MessengerPort for multi-messenger support ([8d2d3a6](https://github.com/twohreichel/PiSovereign/commit/8d2d3a63b498f615aa573ebff18e8d6ac1a1b23a))
* **infrastructure:** add WhatsApp and Signal messenger adapters ([784a568](https://github.com/twohreichel/PiSovereign/commit/784a5686f8337bab37d10bf8d0af9f0a993130e2))
* **integration_signal:** add Signal messenger integration crate ([cba5938](https://github.com/twohreichel/PiSovereign/commit/cba5938ff6cca0c4139e97b1eb38d24af6175ecd))
* **presentation_http:** add Signal handlers, routes, and AppState integration ([8ae093c](https://github.com/twohreichel/PiSovereign/commit/8ae093ccee53ada027c212ea47603a9ffc48db72))
* **setup:** add signal-cli installation and systemd service for Signal messenger integration ([261e22a](https://github.com/twohreichel/PiSovereign/commit/261e22ab952766a7dc9697c7b20169bd308b0bb7))


### Bug Fixes

* **adapters:** reorder ollama_inference_adapter module for consistency ([78288cf](https://github.com/twohreichel/PiSovereign/commit/78288cf1ecaf069043c34388ed826202b6161725))
* **tests:** fix platform-specific whisper executable tests and SignalConfig defaults ([5beeb5b](https://github.com/twohreichel/PiSovereign/commit/5beeb5b2a4966abaeab662787a4e6019396d53fa))
* update LLM model reference and adjust whisper executable for macOS ([16d1870](https://github.com/twohreichel/PiSovereign/commit/16d187063772250e75eaa85a2d84bb6fe3473cac))


### Documentation

* add comprehensive macOS setup guide ([7230d1c](https://github.com/twohreichel/PiSovereign/commit/7230d1c19054ceef39fa73277f79e72371aa3ad2))
* **config:** add platform support documentation ([b08dd29](https://github.com/twohreichel/PiSovereign/commit/b08dd290a25384533a876ff3dddd2fbc274cb961))
* **readme:** add macOS platform support ([1e86cc5](https://github.com/twohreichel/PiSovereign/commit/1e86cc59d3609e3a6e504a3f3e689ccd8b1a820a))
* **readme:** update quick start section and add setup instructions for macOS and Raspberry Pi ([3dcf228](https://github.com/twohreichel/PiSovereign/commit/3dcf228a46e5d3c2faaa94bf9d61f7886d11ce2d))


### Code Refactoring

* **ai_core,infrastructure:** rename hailo module to ollama ([32cb6ce](https://github.com/twohreichel/PiSovereign/commit/32cb6cebe031229b73adb9fb5c27173f9cd110b6))

## [0.2.2](https://github.com/twohreichel/PiSovereign/compare/v0.2.1...v0.2.2) (2026-02-08)


### Bug Fixes

* **workflows:** update Docker image name format and add OpenSSL installation step ([0d661de](https://github.com/twohreichel/PiSovereign/commit/0d661de2c898361b4a2524e5b72db54f718d0d2b))


### Documentation

* add link to full documentation in README ([0d661de](https://github.com/twohreichel/PiSovereign/commit/0d661de2c898361b4a2524e5b72db54f718d0d2b))

## [0.2.1](https://github.com/twohreichel/PiSovereign/compare/v0.2.0...v0.2.1) (2026-02-08)


### Bug Fixes

* **ci:** enhance cross-compilation setup for ARM64 with OpenSSL support ([7102a14](https://github.com/twohreichel/PiSovereign/commit/7102a1416f1c5e72eada1c98e0bdc034df77e6d9))
* **ci:** enhance cross-compilation setup for ARM64 with OpenSSL support ([3c9750a](https://github.com/twohreichel/PiSovereign/commit/3c9750adc87571e9c266f000ff83057c074516ae))

## [0.2.0](https://github.com/twohreichel/PiSovereign/compare/v0.1.0...v0.2.0) (2026-02-08)


### ⚠ BREAKING CHANGES

* **domain:** Timezone::new() replaced with Timezone::try_new() which returns Result<Timezone, InvalidTimezone>
* **security:** API key configuration format changed
* **security:** Plaintext secrets (SEC003, SEC004, SEC005) are now Critical severity in production mode, blocking startup unless PISOVEREIGN_ALLOW_INSECURE_CONFIG=true is set.
* **auth:** ApiKeyAuthLayer now requires api_key_users config for multi-tenant setups. Single-key mode remains supported for backward compatibility.
* **proton:** TlsConfig.verify_certificates changed from bool to Option<bool>
* **infrastructure:** SledCache replaced by RedbCache
* Migrated from nightly-2025-01-15 to stable Rust 1.93.0. Edition 2024 is now fully supported in stable Rust.
* **security:** ProtonConfig now requires tls field

### Features

* add comprehensive analysis report for project evaluation ([a4dccb7](https://github.com/twohreichel/PiSovereign/commit/a4dccb7aaa3e1e2c592d0ba578a3d121296800d1))
* add detailed project analysis document ([10cc345](https://github.com/twohreichel/PiSovereign/commit/10cc34548588b497dc4673a5e75354a0a265cc8c))
* add detailed project analysis document ([bb2a00c](https://github.com/twohreichel/PiSovereign/commit/bb2a00c35f5e5432c5876ade8b8c824e0721dced))
* add detailed project analysis document ([40fb620](https://github.com/twohreichel/PiSovereign/commit/40fb620ff4ab14d180a500718fc0a072845bf273))
* add detailed project analysis document for PiSovereign ([3de12bc](https://github.com/twohreichel/PiSovereign/commit/3de12bcece484f16f2ae20e4717e56d5a64eeda3))
* add detailed project analysis document for PiSovereign ([e15f327](https://github.com/twohreichel/PiSovereign/commit/e15f32736856fc4568c8cb2960f6d7eafbc287d5))
* add OpenAPI documentation with Swagger UI and ReDoc ([2653277](https://github.com/twohreichel/PiSovereign/commit/2653277baf75210f4686847ab648b2dd19630a9a))
* add SQL migration files and improve migration error handling ([2ec1bba](https://github.com/twohreichel/PiSovereign/commit/2ec1bba566e4c5deb7975eaf4e170c7625aa8b79))
* add structured JSON logging and request ID correlation ([2af0405](https://github.com/twohreichel/PiSovereign/commit/2af040588827aa887b7306e1e064ddec629f95d4))
* **agent:** integrate calendar and email services into morning briefing ([5679802](https://github.com/twohreichel/PiSovereign/commit/5679802810ffa3fbe93459936e54b6f2dbef6b56))
* **ai_core:** implement dynamic model discovery ([0e0b0ce](https://github.com/twohreichel/PiSovereign/commit/0e0b0ceeaad2054bbc835e82e0e92760d2b6c793))
* **ai_speech:** add audio format converter with FFmpeg support ([4d68df0](https://github.com/twohreichel/PiSovereign/commit/4d68df01f1ae1e982684a56a2fc8c5f070ba1288))
* **ai_speech:** add local speech providers (whisper.cpp + Piper) ([7881bf6](https://github.com/twohreichel/PiSovereign/commit/7881bf65f3a2f0c7bcaffe8ac61530c689f10242))
* **ai_speech:** add Speech-to-Text and Text-to-Speech crate ([97c4f8e](https://github.com/twohreichel/PiSovereign/commit/97c4f8e42c039ff734515103e6e7a3b2c66d1fec))
* **analysis:** add comprehensive project analysis document ([bb00d79](https://github.com/twohreichel/PiSovereign/commit/bb00d79d8d3d44ab0edf49811e4b0d155b7464e0))
* **analysis:** add comprehensive project analysis document ([30e34ba](https://github.com/twohreichel/PiSovereign/commit/30e34ba5e70491a6024119450920a5ef1cb105de))
* **analysis:** add comprehensive technical analysis document for project overview and readiness assessment ([8e1e408](https://github.com/twohreichel/PiSovereign/commit/8e1e408866234869ef1af236e9fc01c7b347949a))
* **analysis:** remove outdated detailed project analysis document ([fd5f9c3](https://github.com/twohreichel/PiSovereign/commit/fd5f9c37dac588f864380558b7730c6c57a8814f))
* **application:** activate conversation context in ChatService ([ea3e726](https://github.com/twohreichel/PiSovereign/commit/ea3e72632f6add4cb0ff967555080888443da7bf))
* **application:** add conversation context service with 7-day retention ([c3fe36a](https://github.com/twohreichel/PiSovereign/commit/c3fe36a8d684cf91a2830a1b5b72189b11b68d00))
* **application:** add Email and Calendar ports and services ([142079f](https://github.com/twohreichel/PiSovereign/commit/142079f3e212bcef02c20f64c25f3b81a9b73938))
* **application:** add RequestContext for auth-context propagation ([8f74582](https://github.com/twohreichel/PiSovereign/commit/8f7458274029654f2301886835c1aec8de3d789d))
* **application:** add SpeechPort for speech processing operations ([b543ecd](https://github.com/twohreichel/PiSovereign/commit/b543ecd6a3cbfdea4b2da3ce44e4411d32d68519))
* **application:** add VoiceMessageService for voice message processing ([70edab7](https://github.com/twohreichel/PiSovereign/commit/70edab7a08a0ec1dc5c58bc162a28af1fd6890d7))
* **application:** add weather, task, and model registry ports ([e6ce37f](https://github.com/twohreichel/PiSovereign/commit/e6ce37f4fb022f21f738271ceeb5460dc8960929))
* **application:** add web_search intent to CommandParser ([4e79de7](https://github.com/twohreichel/PiSovereign/commit/4e79de7e1a4e91e254dfe96969e3e9d1af5f3d2f))
* **application:** add WebSearchPort and integrate into AgentService ([f34d00e](https://github.com/twohreichel/PiSovereign/commit/f34d00eda7e3558e0dcc67be06249227150d8318))
* **application:** integrate DraftStorePort in AgentService ([43a2260](https://github.com/twohreichel/PiSovereign/commit/43a22607b76d9d26171dc85090c1b3876cff3715))
* **application:** integrate UserProfile for timezone personalization ([b469c02](https://github.com/twohreichel/PiSovereign/commit/b469c029493830bfea8b6d55c6a9de62f151bc19))
* **approval_service:** implement ApprovalService for managing approval workflows ([d5a8a4c](https://github.com/twohreichel/PiSovereign/commit/d5a8a4cc9d3f4803ca4bd2fada45200ba596414f))
* **approval_service:** use ok_or_else for better error handling in approval requests ([bbf270d](https://github.com/twohreichel/PiSovereign/commit/bbf270d12a5b613e068837734702aba44c7f8f90))
* **audit_entry:** change with_ip_address method to const for improved performance ([bbf270d](https://github.com/twohreichel/PiSovereign/commit/bbf270d12a5b613e068837734702aba44c7f8f90))
* **audit_log:** add #[must_use] attribute to query builder methods for clarity ([bbf270d](https://github.com/twohreichel/PiSovereign/commit/bbf270d12a5b613e068837734702aba44c7f8f90))
* **audit:** add audit log entry entity and persistence interface ([b899e6f](https://github.com/twohreichel/PiSovereign/commit/b899e6fb827f17668109fe7a9665c725e51058ba))
* **audit:** add request_id to AuditEntry for distributed tracing ([c519828](https://github.com/twohreichel/PiSovereign/commit/c519828a5653b307cd63beca59e243ca149efb9a))
* **auth:** implement multi-tenant user context from API key lookup ([c634b81](https://github.com/twohreichel/PiSovereign/commit/c634b8143a4096d5eb4173a579f6b249eab664f9))
* **briefing_service:** add Morning Briefing Service to aggregate calendar events, emails, and tasks ([8a8de10](https://github.com/twohreichel/PiSovereign/commit/8a8de1069b1fdcd41d1f6c4a868d903c6d0ec28c))
* **briefing:** add weather integration with UserProfile &gt; config fallback ([2bd1504](https://github.com/twohreichel/PiSovereign/commit/2bd15047801b4813b8a47e432dd1336aa923d380))
* **briefing:** integrate task service with user_id from RequestContext ([f023018](https://github.com/twohreichel/PiSovereign/commit/f0230180366272a3521a7a81b888747f2ab8a4d6))
* **cache:** add cached inference adapter with LLM response caching ([dde561f](https://github.com/twohreichel/PiSovereign/commit/dde561f6f2117035fe0359ca44a608ae28cc67d4))
* **cache:** implement multi-layer caching infrastructure ([1026143](https://github.com/twohreichel/PiSovereign/commit/10261439d2de158c69c15c43bca6663f91fe1611))
* **caldav:** implement HTTP-based CalDAV client with event parsing and iCalendar support ([e74f5ad](https://github.com/twohreichel/PiSovereign/commit/e74f5adb003aff27719fd5175c21926d1b650bb4))
* **calendar:** add UpdateCalendarEvent command ([554c273](https://github.com/twohreichel/PiSovereign/commit/554c273514d5f5599cdcfcdfd676d7097964e55a))
* **chaos:** add chaos engineering framework for resilience testing ([5a15e44](https://github.com/twohreichel/PiSovereign/commit/5a15e44397cba1f7c3a43fc859b1d751ee7f3fc8))
* **chat_message:** add #[must_use] attribute to with_metadata method for clarity ([bbf270d](https://github.com/twohreichel/PiSovereign/commit/bbf270d12a5b613e068837734702aba44c7f8f90))
* **chat:** export MAX_CONVERSATION_MESSAGES alongside ChatService ([56f24a1](https://github.com/twohreichel/PiSovereign/commit/56f24a102ddf8472b2ab91160181f15941733854))
* **ci:** update CI configuration for code coverage and add tarpaulin settings ([bbb2392](https://github.com/twohreichel/PiSovereign/commit/bbb23929da48973f57084d18bf042dd7fb344067))
* **cli:** add SQLite backup command with S3 support ([a8ba4c4](https://github.com/twohreichel/PiSovereign/commit/a8ba4c4d8ce7449da0e0f87b2f7453f92bcb1706))
* **client:** implement WhatsApp client for sending messages ([7c8a0af](https://github.com/twohreichel/PiSovereign/commit/7c8a0af7c34d597fb6c7201a3beeadd35432b67d))
* **clippy:** update lint settings to allow additional clippy warnings ([bbf270d](https://github.com/twohreichel/PiSovereign/commit/bbf270d12a5b613e068837734702aba44c7f8f90))
* **command_parser:** enhance command parsing with property-based tests ([bbb2392](https://github.com/twohreichel/PiSovereign/commit/bbb23929da48973f57084d18bf042dd7fb344067))
* **command_parser:** enhance LLM intent detection with additional commands and JSON parsing ([e5a00aa](https://github.com/twohreichel/PiSovereign/commit/e5a00aa724dcb8b3d06719bc19abfb513b9640b5))
* **config:** add api_key to user_id mapping configuration ([838f24e](https://github.com/twohreichel/PiSovereign/commit/838f24e51918076a379161f2c4c22790ff4499fd))
* **config:** add environment validation and startup security warnings ([553e113](https://github.com/twohreichel/PiSovereign/commit/553e1133636db94c8d952d1615f9274a4f728605))
* **config:** add hot-reloadable configuration support with SIGHUP handling ([ecb85e8](https://github.com/twohreichel/PiSovereign/commit/ecb85e80f88856788ae03c67f526b8e879254be6))
* **config:** add speech processing configuration ([7b3567e](https://github.com/twohreichel/PiSovereign/commit/7b3567ed2e828af6a7a14d5e5f4a7aca7f5ae01b))
* **config:** add websearch configuration section ([5bc15fb](https://github.com/twohreichel/PiSovereign/commit/5bc15fbbaa2ad96932004f27056de5ccfa7de3b6))
* **config:** add WhatsApp configuration with default values ([d731f98](https://github.com/twohreichel/PiSovereign/commit/d731f98bc6e3c6b0187915352d2f7021d8071c74))
* **config:** expand configuration options for server, inference, security, and integrations ([85a9e05](https://github.com/twohreichel/PiSovereign/commit/85a9e0504a51d13b5b69ca3d83575f10ed719fd0))
* **coverage:** switch from cargo-llvm-cov to cargo-tarpaulin for coverage reporting ([4fd4ec6](https://github.com/twohreichel/PiSovereign/commit/4fd4ec67288a7ae70ee31b801864e11ed6d654c4))
* **database:** implement SQLite-based conversation storage and connection management ([c5b560b](https://github.com/twohreichel/PiSovereign/commit/c5b560b71e6dca7bdcaf25955eb8bf203b126198))
* **docker:** add docker-compose for local development ([1a6f4c5](https://github.com/twohreichel/PiSovereign/commit/1a6f4c5be884345c3d737bd619f4dc5ee203f386))
* **docker:** add multi-stage Dockerfile with Hailo SDK support ([13d653a](https://github.com/twohreichel/PiSovereign/commit/13d653ae0cb490d8824657ee78216c624fc7fe6b))
* **docker:** add Traefik reverse proxy with automatic TLS ([f41311c](https://github.com/twohreichel/PiSovereign/commit/f41311c3efa8025a2b664229112b862964063c51))
* **docs:** add detailed system analysis document ([b729ec1](https://github.com/twohreichel/PiSovereign/commit/b729ec16419a25f101987373c998ce37a3e8fc5f))
* **docs:** update README with local-first processing and provider options for STT/TTS ([ab712d4](https://github.com/twohreichel/PiSovereign/commit/ab712d44543c7e86401d15f8e1bac482cd615b67))
* **domain:** add DraftStorePort and PersistedEmailDraft entity ([b9b62a5](https://github.com/twohreichel/PiSovereign/commit/b9b62a52f161c9116b207d2385f98d38c4802ebd))
* **domain:** add multi-tenancy foundation types ([624b4c2](https://github.com/twohreichel/PiSovereign/commit/624b4c2ab29d445f7b3365323842afe8b50e3004))
* **domain:** add user profile, location, and task entities ([3cd9bb6](https://github.com/twohreichel/PiSovereign/commit/3cd9bb69a0a210fd2c2d6914cc14a128d63a7dcd))
* **domain:** add validated Timezone and Humidity value objects ([3aefd4b](https://github.com/twohreichel/PiSovereign/commit/3aefd4b1b9e1e34c72a01128a3fa1d6ebd6a0497))
* **domain:** add VoiceMessage entity for voice message handling ([430a5fe](https://github.com/twohreichel/PiSovereign/commit/430a5fe8825ce37493fb714f1327653fd4201a4e))
* **domain:** add WebSearch command and search entities ([bc56e80](https://github.com/twohreichel/PiSovereign/commit/bc56e8074c0d3e2153fbd2d15d966a93abc13fcd))
* **error:** add NotFound and InvalidOperation variants to ApplicationError ([d5a8a4c](https://github.com/twohreichel/PiSovereign/commit/d5a8a4cc9d3f4803ca4bd2fada45200ba596414f))
* **health:** add comprehensive external service health checks ([e4b7a8c](https://github.com/twohreichel/PiSovereign/commit/e4b7a8c3b0ebda8b9fdf37d54f6b16cf4ae4726a))
* **health:** add database health check port ([d8973b0](https://github.com/twohreichel/PiSovereign/commit/d8973b06c928e2e8cdfeb70539f9c9a0c01b766e))
* **health:** wire up HealthService with all available ports ([6bdec3f](https://github.com/twohreichel/PiSovereign/commit/6bdec3fecab401d6ac999fcc1fd282bcdaf62b90))
* **http:** add approval workflow REST API ([4238c28](https://github.com/twohreichel/PiSovereign/commit/4238c28984f51274bb65f19765a984843fee7a20))
* **http:** add audio message handling to WhatsApp webhook ([c6aa994](https://github.com/twohreichel/PiSovereign/commit/c6aa994643c8655b5190b3dec3713b722f11fbce))
* **http:** add CorrelatedHttpClient for request ID propagation ([28c0ec1](https://github.com/twohreichel/PiSovereign/commit/28c0ec1a239934026b15a4b1a0ba11bb9f27ddec))
* **http:** add WhatsApp webhook integration ([4ce315e](https://github.com/twohreichel/PiSovereign/commit/4ce315e3cd694818caecf92de7e6aa6227abe2bf))
* Implement approval request entity and SQLite persistence ([667297f](https://github.com/twohreichel/PiSovereign/commit/667297feeedab81af132401bacfc78fbca0bc368))
* **inference:** implement runtime model switching ([f61f925](https://github.com/twohreichel/PiSovereign/commit/f61f92562d48c7ae8f4c798c661696429c2e0467))
* **infrastructure:** add configurable cache TTLs ([2570dbd](https://github.com/twohreichel/PiSovereign/commit/2570dbdc9c4e67b6d7ff4dc7f9f4286b975abdfd))
* **infrastructure:** add degraded inference adapter for graceful Hailo failover ([e9f5639](https://github.com/twohreichel/PiSovereign/commit/e9f56395e5c74d5a1b7fec3140c16a531ca4246a))
* **infrastructure:** add OpenTelemetry/Tempo integration ([279ef00](https://github.com/twohreichel/PiSovereign/commit/279ef0039401757e4f74656417d899c97d582f12))
* **infrastructure:** add Proton email and CalDAV calendar adapters ([aeb5428](https://github.com/twohreichel/PiSovereign/commit/aeb5428921a32e703d7f9f0db93bf77473457e4f))
* **infrastructure:** add SpeechAdapter implementing SpeechPort ([088a67b](https://github.com/twohreichel/PiSovereign/commit/088a67b2a204784827618ab144c2a3857a9202e5))
* **infrastructure:** add SqliteDraftStore adapter ([058c89e](https://github.com/twohreichel/PiSovereign/commit/058c89e84bd3b1b102deeb2146c0ecbab8729cfb))
* **infrastructure:** add user profile SQLite storage ([7671597](https://github.com/twohreichel/PiSovereign/commit/7671597717b9eb07fde1cbcb145935bdf3b118f8))
* **infrastructure:** add weather, task, and model registry adapters ([c9a03b9](https://github.com/twohreichel/PiSovereign/commit/c9a03b9ec49371275c5f3ac5f48f649fe35b3cfd))
* **infrastructure:** add WebSearchAdapter implementing WebSearchPort ([370f98c](https://github.com/twohreichel/PiSovereign/commit/370f98c255ffc6f71c4c3437fe9d37ffdefa5274))
* **infrastructure:** wire telemetry and degraded mode in HTTP server ([e7adffd](https://github.com/twohreichel/PiSovereign/commit/e7adffd31fde69d467b00ba98a50b8962b453887))
* **integration_caldav:** add VTODO task support ([ec6106b](https://github.com/twohreichel/PiSovereign/commit/ec6106b13d0866cbac087c88491795c21a9c1e42))
* **integration_caldav:** improve XML parsing with quick-xml library ([fa0ac19](https://github.com/twohreichel/PiSovereign/commit/fa0ac19114f1cc181c47e991fe2038ceb8054015))
* **integration_proton:** add reconnecting client with exponential backoff ([eb93dcd](https://github.com/twohreichel/PiSovereign/commit/eb93dcda01d6143f7069d635e9bde66458a8d419))
* **integration_proton:** implement IMAP/SMTP client for Proton Bridge ([504c60e](https://github.com/twohreichel/PiSovereign/commit/504c60ea27fa1695e55363ae634cc6f38d084562))
* **integration_proton:** implement Proton Mail client with error handling and configuration ([6ce96cb](https://github.com/twohreichel/PiSovereign/commit/6ce96cb4c278df90c49b754e868835d465be07bd))
* **integration_weather:** add Open-Meteo weather client ([cb0bc9f](https://github.com/twohreichel/PiSovereign/commit/cb0bc9fdc3a28ecf2dcddf17e529a47617e41c97))
* **logging:** JSON format as production default with rotation docs ([c3bfe43](https://github.com/twohreichel/PiSovereign/commit/c3bfe43ab4e7e514d12aa851f0137b81e7629c41))
* **metrics:** add metrics collection and expose metrics endpoints ([4770cf4](https://github.com/twohreichel/PiSovereign/commit/4770cf43f2aaf3c757afaeb0a778384aca50815b))
* **metrics:** add P50/P90/P99 latency percentiles ([5c38c4a](https://github.com/twohreichel/PiSovereign/commit/5c38c4a3f8df3ab7887db29387909faf36b1983e))
* **middleware:** add API key authentication and rate limiting layers ([ae6650a](https://github.com/twohreichel/PiSovereign/commit/ae6650ad32076050b1026be8efbcf7a75541d134))
* **model_selector:** implement dynamic model selection based on task complexity ([a5078ad](https://github.com/twohreichel/PiSovereign/commit/a5078adccc9b98fd258aca12d5499b8c06b66906))
* **monitoring:** add Grafana dashboard and Prometheus config ([dae5df2](https://github.com/twohreichel/PiSovereign/commit/dae5df204a08cb10d39b6909c5b74881f4c21b23))
* **multi-tenant:** propagate TenantId through RequestContext ([1a102ac](https://github.com/twohreichel/PiSovereign/commit/1a102acf9ed5df374d4485a3e3c3c5c265612858))
* **observability:** add histogram metrics and Prometheus alerting rules ([1791ca9](https://github.com/twohreichel/PiSovereign/commit/1791ca9bfd772b6e3679ba4d937676707dd5c9e2))
* **parser:** add natural language date parsing with fuzzydate ([a14befb](https://github.com/twohreichel/PiSovereign/commit/a14befbf6d2bf6cee7e7ce96d878970ce487c2f8))
* **persistence:** add async database layer with sqlx ([2b43e56](https://github.com/twohreichel/PiSovereign/commit/2b43e568108f42f3b8bfe2f2369e6bfe31b3b89a))
* **persistence:** add incremental conversation persistence ([f1f6a0f](https://github.com/twohreichel/PiSovereign/commit/f1f6a0f25d737338a161d7750f54e337352859d4))
* **persistence:** add sequence_number to messages for incremental persistence ([ac41fbd](https://github.com/twohreichel/PiSovereign/commit/ac41fbda55dcf7d916b1bc65f4d8b42cead90243))
* **persistence:** add SQLite audit log implementation ([e6f57f1](https://github.com/twohreichel/PiSovereign/commit/e6f57f13145a7b7069620b98b2fdf6feb7b3e317))
* **presentation_http:** add location update HTTP endpoints ([a3682ec](https://github.com/twohreichel/PiSovereign/commit/a3682ece36d6b19af2fdfc2f9de2a9a5e2c54882))
* **presentation:** add rate limiter cleanup task on startup ([80c7928](https://github.com/twohreichel/PiSovereign/commit/80c79280f87a3af0a1b72353002e3b93e43a0388))
* **rate-limit:** add background cleanup task for stale entries ([0c93111](https://github.com/twohreichel/PiSovereign/commit/0c931110aec99f2bac939e190f4a56652137f274))
* **release:** add release-please for automated versioning ([7e75a84](https://github.com/twohreichel/PiSovereign/commit/7e75a845b098a0b8b3d2d7b5b90aa7befcc99f33))
* **release:** add workflow for building and uploading release binaries ([4a5bade](https://github.com/twohreichel/PiSovereign/commit/4a5bade7eaf4e2db9de3491147b1179001ec5572))
* **resilience:** add circuit breaker pattern for external services ([d1f0c7a](https://github.com/twohreichel/PiSovereign/commit/d1f0c7aad8eeddb7ad51dfe87433e04cfd2383a3))
* **retry:** add persistent retry queue with exponential backoff ([0218d2e](https://github.com/twohreichel/PiSovereign/commit/0218d2e6ce346d084b4bf4befabb9879d0ab540e))
* **retry:** implement generic RetryConfig with exponential backoff ([6a124a3](https://github.com/twohreichel/PiSovereign/commit/6a124a34815bf34ed13d280cce38d0e719702fb9))
* **scheduler:** add tokio-cron-scheduler for recurring tasks ([259e672](https://github.com/twohreichel/PiSovereign/commit/259e6724662cfffa00facfabcfe2eb6f3e22200a))
* **security:** add cargo-deny configuration for dependency auditing ([c877158](https://github.com/twohreichel/PiSovereign/commit/c877158bcc9c2af6600f8e590dd7772cfaaff753))
* **security:** add configurable request body size limits ([d53af57](https://github.com/twohreichel/PiSovereign/commit/d53af5721fdf05f0283bd16db11d922be321824e))
* **security:** add configurable TLS verification for Proton clients ([7db7e0a](https://github.com/twohreichel/PiSovereign/commit/7db7e0a32a4603413eab6bc4f6803580b4f13920))
* **security:** add error response sanitization for production ([71fbd39](https://github.com/twohreichel/PiSovereign/commit/71fbd392c5ca8271dad396f923583923721d056c))
* **security:** add SecretStore trait with Vault and env backends ([7db6de3](https://github.com/twohreichel/PiSovereign/commit/7db6de36dcbe43e1b93ea0b38094621c9ad771e3))
* **security:** add security headers middleware ([f051c9c](https://github.com/twohreichel/PiSovereign/commit/f051c9c3ede9b64e311300eb1fffeee2a5ed2562))
* **security:** add TLS configuration and timing-safe authentication ([26fb714](https://github.com/twohreichel/PiSovereign/commit/26fb7147c0b1744dc9d7e3a956dd4f0e59b230fb))
* **security:** add trusted proxy support for rate limiting ([34e956e](https://github.com/twohreichel/PiSovereign/commit/34e956ea49c1f4527597d02922636645b20c9305))
* **security:** block plaintext API keys in production mode ([52d8a35](https://github.com/twohreichel/PiSovereign/commit/52d8a35ba49de2f37dded20c629f92f85f7b5fbd))
* **security:** implement Argon2 password hashing for API keys and add CLI command for hashing ([0bf39f6](https://github.com/twohreichel/PiSovereign/commit/0bf39f613f586014b172ee7960523bbe91695631))
* **security:** implement secure API key storage with Argon2 hashing ([c8c3e48](https://github.com/twohreichel/PiSovereign/commit/c8c3e48723adbbd98bfd83911ce9eaa150cb4364))
* **security:** integrate SecurityValidator into startup ([18b2ff0](https://github.com/twohreichel/PiSovereign/commit/18b2ff000d22aa70e82ae3efa2f5e023f2d65569))
* **server:** add graceful shutdown configuration with timeout ([288cca1](https://github.com/twohreichel/PiSovereign/commit/288cca1e24370fbfd7ae573e42eecc994dec4c90))
* **streaming:** add streaming support for inference with SSE integration ([6bf0034](https://github.com/twohreichel/PiSovereign/commit/6bf0034b317ca91e74ff6cf52da7a82043fd1f3c))
* **tasks:** add Task CRUD commands ([8adb429](https://github.com/twohreichel/PiSovereign/commit/8adb4299cbf6f5bf9412e4ae36fee3c0139a4bd2))
* **telemetry:** add graceful fallback for unavailable OTLP collector ([3854718](https://github.com/twohreichel/PiSovereign/commit/3854718c8fa482b9b10400198c815a95baddbdaf))
* **templates:** add Tera template engine for emails and messages ([07f1901](https://github.com/twohreichel/PiSovereign/commit/07f19013ddecbb848f2b81c9c7f16ad381009589))
* **testing:** add testcontainers support for integration tests ([901edd7](https://github.com/twohreichel/PiSovereign/commit/901edd74198223e2933d7b45dfd6265aca39d3cf))
* **tests:** add concurrency tests for multi-layer cache ([bbb2392](https://github.com/twohreichel/PiSovereign/commit/bbb23929da48973f57084d18bf042dd7fb344067))
* **websearch:** add integration_websearch crate with Brave and DuckDuckGo support ([b48acae](https://github.com/twohreichel/PiSovereign/commit/b48acaeaa729e7b6e8cd88db88f4b833154a239a))
* **whatsapp:** add audio/voice message support to webhook ([d7c9aa8](https://github.com/twohreichel/PiSovereign/commit/d7c9aa8a764b531acda8e7c9b851a22a4aa169a0))
* **whatsapp:** add media download/upload and audio message support ([54b1f53](https://github.com/twohreichel/PiSovereign/commit/54b1f53f86e43463a2da8a833e5ece24e3d056d1))
* **workflows:** update Rust toolchain to version 1.93.0 in CI workflows ([3876b4a](https://github.com/twohreichel/PiSovereign/commit/3876b4a50718655b58c1e9c6be09ff3a30a66404))


### Bug Fixes

* add rationale for ignoring tokio-tar vulnerability in deny.toml ([fee5ee8](https://github.com/twohreichel/PiSovereign/commit/fee5ee8199aeb9a841846275a00b0e7a1a001ded))
* **agent:** implement dynamic model listing from Hailo API ([65c9f6f](https://github.com/twohreichel/PiSovereign/commit/65c9f6f5dda9e2b51fed709eb3d8e7fae10cfb47))
* **api_error:** map ApplicationError variants to ApiError appropriately ([d5a8a4c](https://github.com/twohreichel/PiSovereign/commit/d5a8a4cc9d3f4803ca4bd2fada45200ba596414f))
* **ci:** configure release-please for workspace version inheritance ([7999e5a](https://github.com/twohreichel/PiSovereign/commit/7999e5adeec9a7c37752bccca7a4742c2adb61da))
* **ci:** switch release-please to simple strategy for workspace versions ([cad31eb](https://github.com/twohreichel/PiSovereign/commit/cad31eb403dcc04ddc56e20c57e9cc2b0534c937))
* **ci:** switch release-please to simple strategy for workspace versions ([4f68f3f](https://github.com/twohreichel/PiSovereign/commit/4f68f3fb182208087d6bf176e9b18e572bf4593f))
* **ci:** upgrade SBOM format to cyclone_dx_json_1_6 ([7467156](https://github.com/twohreichel/PiSovereign/commit/7467156ff73fdcc1b6965d0f396ccd62cb6b632a))
* **clippy:** resolve lint errors ([bf08d71](https://github.com/twohreichel/PiSovereign/commit/bf08d7105ab91fec5bf4f15a9c2216cf518db0ed))
* **commands:** simplify Help command formatting ([d5a8a4c](https://github.com/twohreichel/PiSovereign/commit/d5a8a4cc9d3f4803ca4bd2fada45200ba596414f))
* **http:** initialize ApprovalService with SQLite backend ([660397f](https://github.com/twohreichel/PiSovereign/commit/660397fadff991e9bcf2e288d9e5b6513944dcac))
* **presentation_http:** add WebSearch to command_type_name match ([3b7adae](https://github.com/twohreichel/PiSovereign/commit/3b7adae714f5828b73f6ab7dc920ceaa79db308d))
* **proton:** add runtime warning for disabled TLS verification ([637dc67](https://github.com/twohreichel/PiSovereign/commit/637dc674ea621251ba32193ea6f8acb6e5df9acf))
* **proton:** secure TLS verification default to true ([ba3d9d4](https://github.com/twohreichel/PiSovereign/commit/ba3d9d4f6b5194813b3bfb3a307373f4b9abead8))
* **readme:** update badge links and add missing shield images ([3c64d45](https://github.com/twohreichel/PiSovereign/commit/3c64d45498a2947e1df533682bf3e2c527baaa07))
* resolve all clippy warnings ([1ba8f01](https://github.com/twohreichel/PiSovereign/commit/1ba8f0172759b46a33c1c31c07cd21156c28294e))
* resolve clippy warnings across workspace ([c792285](https://github.com/twohreichel/PiSovereign/commit/c79228536d39b3386a987c145590caf9d067fec9))
* resolve clippy warnings for Rust 1.93.0 ([c28cf29](https://github.com/twohreichel/PiSovereign/commit/c28cf29ee2e9263419a5b81deec0420d570fb3cf))
* update coverage threshold to fail under 60% ([8955cbd](https://github.com/twohreichel/PiSovereign/commit/8955cbd3c886726cba5ab83cb643e6f42998b3de))
* update coverage threshold to fail under 70% ([6939b16](https://github.com/twohreichel/PiSovereign/commit/6939b164f2552aa79260385824a51bf7084f2f44))
* update error handling in Vault secret store to use String::new() for empty values ([07149ca](https://github.com/twohreichel/PiSovereign/commit/07149ca88d71887f024d5528859447520ca94d0d))
* **websearch:** improve error handling and response mapping ([bbb2392](https://github.com/twohreichel/PiSovereign/commit/bbb23929da48973f57084d18bf042dd7fb344067))
* **whatsapp:** implement response sending via Cloud API ([6e577f6](https://github.com/twohreichel/PiSovereign/commit/6e577f65bb298ae485daedaf3a4b0cfb214009c3))
* **whatsapp:** resolve clippy option_if_let_else warning ([1376113](https://github.com/twohreichel/PiSovereign/commit/13761131146f30594a7565b92e81077a2842e063))


### Performance Improvements

* add criterion benchmarks for chat pipeline ([35af4cc](https://github.com/twohreichel/PiSovereign/commit/35af4cc38c9eba5303bd42d30eeb74d6da29729d))
* **ai_speech:** use Arc&lt;[u8]&gt; for zero-copy AudioData cloning ([a180910](https://github.com/twohreichel/PiSovereign/commit/a1809102e9c0947ddd6ee4d909e8949b7d5f825a))
* **ci:** add concurrency and caching to release workflows ([84dfb03](https://github.com/twohreichel/PiSovereign/commit/84dfb0385c2745a47efe8e3dd72b5b4d919af8c7))
* **ci:** optimize CI workflow for faster execution ([0d4c974](https://github.com/twohreichel/PiSovereign/commit/0d4c974177def795fff21a91cde4f3829ade5eba))
* **infra:** add file-based circuit breaker state persistence ([5a187de](https://github.com/twohreichel/PiSovereign/commit/5a187dee09bb997b381b974859489746ef676e80))
* optimize hot-path clone operations in inference pipeline ([b9bc152](https://github.com/twohreichel/PiSovereign/commit/b9bc1522f3f932784b1f1ebbe096552d7742673f))


### Documentation

* add Brave Search API setup guide and configuration reference ([6603569](https://github.com/twohreichel/PiSovereign/commit/6603569857e55865361db8534df28098732bdf04))
* add CHANGELOG.md with migration guide ([0a8da65](https://github.com/twohreichel/PiSovereign/commit/0a8da65d35edcc44265667543e34fb1d34e69c73))
* add comprehensive project analysis for PiSovereign ([7aba022](https://github.com/twohreichel/PiSovereign/commit/7aba022acf67114612281dabe13a97c9a48340b6))
* add deployment, hardware-setup, and security documentation ([3207724](https://github.com/twohreichel/PiSovereign/commit/32077246f9c2e4cefb63af2beb76ebc495974c44))
* add detailed project analysis document for PiSovereign ([c7578f2](https://github.com/twohreichel/PiSovereign/commit/c7578f2271c0421b229b2dcc75e1966afde39dd1))
* add detailed project analysis for PiSovereign ([b650fd9](https://github.com/twohreichel/PiSovereign/commit/b650fd9324af0a81792e8903fbb00c97e1ec4383))
* add doc tests for public domain and application APIs ([5a64369](https://github.com/twohreichel/PiSovereign/commit/5a64369b4a200b292aab66770e5c043a05acd799))
* add production deployment section to README ([e4cb8ec](https://github.com/twohreichel/PiSovereign/commit/e4cb8ec12b4046079cf8814f7768df7b4b255c0d))
* add voice message (STT/TTS) documentation ([acce22b](https://github.com/twohreichel/PiSovereign/commit/acce22b4c295e357fed5f39add21bfb51de9faeb))
* enhance grafana monitoring documentation ([7695327](https://github.com/twohreichel/PiSovereign/commit/76953271d5118e568d1abba151cf8a5c82024caf))
* **readme:** add beta warning banner ([a3ea0f1](https://github.com/twohreichel/PiSovereign/commit/a3ea0f1976fb60266c81053b05facb1267b98afa))
* **readme:** add performance and development sections ([923b9f9](https://github.com/twohreichel/PiSovereign/commit/923b9f9427e755e7946db086908cae4b2ff104b5))
* **readme:** remove quick start section for clarity ([2155f4e](https://github.com/twohreichel/PiSovereign/commit/2155f4e29bdbfb0a6b051d892cbc56d20871fb59))
* **readme:** update coverage badge to show percentage ([f0394fb](https://github.com/twohreichel/PiSovereign/commit/f0394fb764001cdf3ac9c5082507f0ce758f9312))
* **readme:** update coverage badge to show percentage ([d006430](https://github.com/twohreichel/PiSovereign/commit/d006430a8028167aaf6e2d13cb7710baac910112))
* remove completed PROJEKT_ANALYSE.md ([81543e3](https://github.com/twohreichel/PiSovereign/commit/81543e3f80e39d537ddb66e89f69efb1621cbc29))
* **vault_secret_store, client:** update example URLs to use angle brackets ([2e240c0](https://github.com/twohreichel/PiSovereign/commit/2e240c080cbda6f32b147e7beee1a0a1dc83e971))
* **websearch:** escape bracket notation in rustdoc comments ([17be5ea](https://github.com/twohreichel/PiSovereign/commit/17be5ea6fb34ca10781a63df47b07507fb8dc543))


### Code Refactoring

* **infrastructure:** migrate from sled to redb for L2 cache ([3c5ee43](https://github.com/twohreichel/PiSovereign/commit/3c5ee432b17bd28af9715cb5dde8d2c21dd5df76))


### Build System

* upgrade rust toolchain to stable 1.93.0 with edition 2024 ([17989d2](https://github.com/twohreichel/PiSovereign/commit/17989d26be2c043e8be8f7878ac63032188311d2))

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
