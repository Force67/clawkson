# OpenClaw Feature Parity Plan for Clawkson

## Context

Clawkson already has strong fundamentals (multi-agent orchestration, knowledge bases, container sandbox, skills, scheduling, webhooks, 7 connector types, 4 LLM providers). OpenClaw offers 25+ messaging channels, 35+ LLM providers, a plugin system, context engine with auto-compaction, voice/speech, Canvas/A2UI, companion apps, MCP support, and more. This plan brings every OpenClaw feature into Clawkson, using an optionally-loadable plugin system for features that diverge from the core.

**Key architectural decision**: Compiled-in plugins via Cargo feature flags as the primary mechanism (avoids Rust ABI instability), with a dynamic library `.so/.dylib` escape hatch for third-party plugins. Every plugin is a separate crate under `crates/plugins/` that can be feature-gated or compiled as a cdylib.

---

## Phase 1: Plugin System Foundation (XL)

**Goal**: Build the plugin infrastructure everything else depends on.

### 1A: Core Plugin Trait Crate — `crates/plugin/`

New crate with zero deps on `clawkson-api` (only `clawkson-core` + `denkwerk`).

**Key traits:**

| Trait | Purpose | Extension Point |
|-------|---------|-----------------|
| `ClawksonPlugin` | Lifecycle: `init()`, `shutdown()`, `manifest()` | Base trait all plugins implement |
| `ToolProvider` | `fn tools(&self, ctx) -> Vec<DynKernelFunction>` | Register new tools |
| `ChannelProvider` | `start()`, `stop()`, `send_message()`, config_schema | New messaging channels |
| `LlmProviderFactory` | `build(config) -> Box<dyn LLMProvider>` | New LLM backends |
| `RouteProvider` | `fn routes() -> Router`, `fn prefix() -> &str` | New API routes |
| `SearchProvider` | `async fn search(query, max) -> Vec<SearchResult>` | New web search backends |
| `ContextEnginePlugin` | `on_ingest`, `on_assemble`, `on_compact`, `after_turn` | Context pipeline hooks |

Plus: `PluginManifest` (name, version, description, dependencies, capabilities), `PluginContext` (db, config, data_dir, event_bus), `PluginCapability` enum.

### 1B: Plugin Registry — `crates/plugin/src/registry.rs`

`PluginRegistry` holds `HashMap<String, Arc<dyn ClawksonPlugin>>` plus typed sub-registries for each trait. Provides:
- `register_plugin()` — calls `init()`, inspects capabilities, populates sub-registries
- `tools_for_context()` — aggregates tools from all ToolProviders
- `build_llm_provider(type_name, config)` — delegates to correct factory
- `plugin_routes()` — merges all RouteProviders into a single Router

### 1C: Dynamic Loading — `crates/plugin/src/ffi.rs`

`libloading`-based `.so/.dylib` loader with C ABI entry point (`clawkson_plugin_create`). Secondary mechanism — primary is compiled-in via feature flags.

### 1D: Open Hardcoded Enums

- `ConnectorType` and `LlmProviderType` → stored as `TEXT` in PostgreSQL (currently closed Rust enums)
- Keep existing variants as constants for backward compat
- `build_provider()` in `llm.rs` becomes a registry lookup instead of a closed match

### 1E: Plugin-Aware AppState

Add `plugins: Arc<PluginRegistry>` to `AppState`.

### 1F: Plugin Migrations

New `plugin_migrations` table tracks per-plugin migration state. Each plugin crate has its own `migrations/` dir, applied at init via raw SQL.

**Files to create:**
- `crates/plugin/Cargo.toml`, `src/lib.rs`, `src/registry.rs`, `src/ffi.rs`, `src/event_bus.rs`
- `crates/plugin/src/traits/` — one file per extension trait
- `crates/db/migrations/0035_extensible_types.sql` — enum → TEXT
- `crates/db/migrations/0036_plugin_migrations.sql`

**Files to modify:**
- `Cargo.toml` (workspace) — add `crates/plugin`
- `crates/api/Cargo.toml` — depend on `clawkson-plugin`
- `crates/api/src/state.rs` — add `plugins` field
- `crates/server/src/main.rs` — init PluginRegistry, load plugins, merge routes
- `crates/api/src/routes/mod.rs` — merge `plugin_routes()` into `api_router()`
- `crates/api/src/routes/conversations.rs` — refactor `build_tool_registry_inner()` to call `plugins.tools_for_context()`
- `crates/api/src/llm.rs` — refactor `build_provider()` to use registry
- `crates/core/src/models.rs` — make `ConnectorType`/`LlmProviderType` extensible (String-backed)
- `crates/db/src/connector.rs`, `crates/db/src/llm_connector.rs` — match new types

---

## Phase 2: Context Engine & Conversation Enhancements (L)

**Goal**: Smart context management, auto-compaction, chat commands, loop detection, retry/failover.

**Depends on**: Phase 1

### 2A: Context Engine Pipeline

Replace current simple `truncate_history()` in `conversations.rs:1612` with a 4-stage pipeline using `ContextEnginePlugin` hooks:

1. **Ingest** — before saving user message (entity extraction, intent detection)
2. **Assemble** — after loading history (inject memories, daily logs, RAG context)
3. **Compact** — when tokens > 80% budget (summarize old messages via cheap LLM)
4. **AfterTurn** — after assistant response (memory updates, daily log append)

### 2B: Auto-Compaction (built-in ContextEnginePlugin)

When context exceeds threshold: summarize older messages keeping last N verbatim, replace with `[system]` summary message, emit SSE `{"type":"compacted"}`.

### 2C: Chat Commands

Parse `/` commands before LLM call: `/compact`, `/status`, `/new`, `/reset`, `/think` (extended reasoning), `/verbose`, `/usage`. Return synthetic assistant messages.

### 2D: Loop Detection

Track `(tool_name, arg_hash)` in a sliding window. After 3 identical calls in 60s, inject warning. After 5, abort generation.

### 2E: Retry/Failover

- Exponential backoff on 429/5xx (up to 3 retries)
- Model failover: primary connector → subtask connector → error
- Queue mode: when agent is busy, queue incoming messages

**Files to create:**
- `crates/api/src/context_engine.rs`, `crates/api/src/compaction.rs`
- `crates/api/src/chat_commands.rs`, `crates/api/src/loop_detector.rs`
- `crates/db/migrations/0037_conversation_compaction.sql`

**Files to modify:**
- `crates/api/src/routes/conversations.rs` — integrate pipeline into `chat_stream`
- `crates/api/src/llm.rs` — retry/backoff, failover, loop detection

---

## Phase 3: Frontend Plugin System (L)

**Goal**: Dynamic UI extension from plugins — pages, sidebar items, settings, connector cards.

**Depends on**: Phase 1

### 3A: Plugin Manifest API

`GET /api/plugins` returns loaded plugins with frontend manifest (sidebar_items, routes, settings_panels, connector_cards, bundle_urls).

### 3B: Dynamic Route Registration

`App.tsx` fetches plugin manifests on load, registers plugin routes via `React.lazy()` + dynamic `import()`.

### 3C: Dynamic Sidebar

`Sidebar.tsx` gains `usePluginNav()` hook — merges plugin nav items into `NAV_GROUPS` by group name. Icons specified as lucide-react icon name strings with a lookup map.

### 3D: Plugin Component SDK — `apps/web/src/lib/plugin-sdk.ts`

Shared API for plugin UIs: `useClawksonApi()`, `useTheme()`, `useAuth()`, plus re-exported Card/Button/PageHeader components.

### 3E: Settings & Connector Extension

Settings page gets "Plugins" section with enable/disable + per-plugin settings panels. Connectors page renders plugin-provided connector cards via generic `PluginConnectorCard`.

**Files to create:**
- `crates/api/src/routes/plugins.rs` — manifest API, UI bundle serving
- `apps/web/src/lib/plugin-sdk.ts`, `apps/web/src/lib/usePlugins.ts`
- `apps/web/src/components/PluginPage.tsx`, `apps/web/src/components/PluginPanel.tsx`
- `crates/db/migrations/0038_plugin_settings.sql`

**Files to modify:**
- `apps/web/src/App.tsx` — dynamic routes
- `apps/web/src/components/Sidebar.tsx` — plugin nav items
- `apps/web/src/pages/Settings.tsx` — plugins section
- `apps/web/src/pages/Connectors.tsx` — plugin connector cards

---

## Phase 4: Core Feature Enhancements (L)

**Goal**: Implement OpenClaw features that belong in the core codebase.

**Depends on**: Phase 1, Phase 2

### 4A: Enhanced Memory

- **Daily logs**: End-of-day markdown summaries in agent memory KB
- **Curated MEMORY.md**: `memory_write` tool for agent to persist explicit long-term notes
- **Hybrid BM25 + vector search**: Add `tsvector` column to `knowledge_entries`, combine with VectorChord via reciprocal rank fusion
- **MMR re-ranking**: Reduce redundancy in retrieved context

### 4B: Multi-Agent Routing

- Deterministic binding rules (pattern match → route to agent)
- Agent-to-agent message queue (shared within conversation)
- Per-sub-agent workspace isolation

### 4C: Diff/Patch Tools

New KernelFunction impls: `apply_diff`, `search_and_replace`, `edit_file` (line-range based).

### 4D: Link Understanding

`fetch_url` tool: reqwest fetch → HTML readability extraction → structured content to LLM.

### 4E: Enhanced Scheduling

Add to existing scheduler: one-shot timers, interval tasks, heartbeat (run if no user activity), standing orders (persistent background goals).

### 4F: Reactions & Polls

- `message_reactions` table + REST endpoints + emoji picker in chat UI
- `polls`/`poll_options`/`poll_votes` tables + agent tool + inline poll rendering

**Files to create:**
- `crates/api/src/tools/memory_write.rs`, `diff_patch.rs`, `fetch_url.rs`
- `crates/db/src/reaction.rs`, `crates/db/src/poll.rs`
- `crates/api/src/routes/reactions.rs`, `crates/api/src/routes/polls.rs`
- Migrations: `0039_hybrid_search.sql`, `0040_reactions.sql`, `0041_polls.sql`, `0042_enhanced_scheduling.sql`

**Files to modify:**
- `crates/api/src/memory.rs` — daily logs, curated memory
- `crates/api/src/embeddings.rs` — hybrid search, MMR
- `crates/api/src/subtask.rs` — agent routing, inter-agent messaging
- `crates/api/src/scheduler.rs` — new task types
- `crates/api/src/routes/conversations.rs` — register new tools
- `apps/web/src/pages/Conversations.tsx` — reactions/polls UI
- `apps/web/src/lib/api.ts` — new API functions

---

## Phase 5: Channel Plugins (L)

**Goal**: All OpenClaw messaging channels as separate plugin crates.

**Depends on**: Phase 1, Phase 3

### 5A: Refactor Telegram to Plugin (reference implementation)

Move `crates/api/src/telegram.rs` + `TelegramManager` → `crates/plugins/channels/telegram/`. Establishes the pattern.

### 5B: Channel Plugin Crates

| Plugin | Crate Path | Size | Key Dependency |
|--------|-----------|------|----------------|
| Telegram | `crates/plugins/channels/telegram` | M | teloxide |
| Discord | `crates/plugins/channels/discord` | M | serenity |
| WhatsApp | `crates/plugins/channels/whatsapp` | L | Node sidecar (Baileys) |
| Signal | `crates/plugins/channels/signal` | M | signal-cli subprocess |
| iMessage | `crates/plugins/channels/imessage` | M | BlueBubbles API (macOS) |
| IRC | `crates/plugins/channels/irc` | S | irc crate |
| Matrix | `crates/plugins/channels/matrix` | M | matrix-sdk |
| MS Teams | `crates/plugins/channels/teams` | M | MS Graph API |
| LINE | `crates/plugins/channels/line` | S | REST webhook |
| Mattermost | `crates/plugins/channels/mattermost` | S | REST + websocket |
| Nostr | `crates/plugins/channels/nostr` | M | nostr-sdk |
| Google Chat | `crates/plugins/channels/google-chat` | S | REST webhook |

Each implements `ChannelProvider`, has own `migrations/`, frontend connector card component.

### 5C: Channel Template

`crates/plugins/channels/_template/` — boilerplate Cargo.toml, lib.rs skeleton, migrations dir, frontend template.

---

## Phase 6: Provider & Tool Plugins (M)

**Goal**: LLM providers, search providers, and specialized tools as plugins.

**Depends on**: Phase 1

### 6A: LLM Provider Plugins

**Generic OpenAI-compatible factory** (`crates/plugins/providers/openai-compat/`) covers ~80% of providers with just config (base_url + headers). Separate crates for non-compatible:

| Plugin | Notes |
|--------|-------|
| `openai-compat` | Generic factory for Groq, Together, Fireworks, DeepSeek, Mistral, etc. |
| `anthropic` | Claude direct API |
| `bedrock` | aws-sdk-bedrockruntime |
| `vertex` | Google Cloud Vertex AI |
| `cohere` | Custom API format |

### 6B: Search Provider Plugins

| Plugin | Crate Path |
|--------|-----------|
| Brave Search | `crates/plugins/search/brave` |
| Perplexity | `crates/plugins/search/perplexity` |
| Firecrawl | `crates/plugins/search/firecrawl` |
| Google Search | `crates/plugins/search/google` |
| DuckDuckGo | `crates/plugins/search/duckduckgo` |

### 6C: MCP Bridge Plugin — `crates/plugins/tools/mcp-bridge/`

Implements `ToolProvider`. Manages connections to MCP servers (stdio/HTTP), translates MCP tools → KernelFunction. Settings panel for server config. Hot-reload on server list changes.

### 6D: Image Generation Plugin — `crates/plugins/tools/image-gen/`

`generate_image` and `edit_image` tools via OpenAI DALL-E, FAL, or Stability AI. Settings panel for provider/key selection.

### 6E: Voice/Speech Plugin — `crates/plugins/tools/voice/`

TTS (ElevenLabs, OpenAI), STT (Whisper), wake word detection, talk mode. Frontend audio components. Most complex plugin — requires WebSocket for real-time audio streaming.

---

## Phase 7: Advanced Features (L)

**Goal**: Canvas/A2UI, companion app APIs, node system, enhanced browser.

**Depends on**: Phase 1, Phase 3, Phase 4

### 7A: Canvas / A2UI Plugin — `crates/plugins/tools/canvas/`

New `/canvas` page with freeform agent-driven workspace. Agent tools: `canvas_create_element`, `canvas_update_element`, `canvas_layout`. Real-time sync via SSE. Frontend uses React Flow or similar.

### 7B: Enhanced Browser Automation

Upgrade `crates/api/src/browser_tools.rs`: direct CDP control, multi-profile, cookie persistence, visual analysis (screenshot → vision model), network interception.

### 7C: Companion App API — `crates/api/src/routes/companion.rs`

Routes for macOS/iOS/Android companion apps: `/api/companion/quick-chat`, `/api/companion/push-subscribe`, `/api/companion/media-upload`. WebSocket endpoint for real-time bidirectional.

### 7D: Node System Plugin — `crates/plugins/tools/node-system/`

Device commands: camera, screen record, clipboard, location, filesystem. Runs via companion daemon. Per-capability user approval.

---

## Phase Dependency Graph

```
Phase 1 (Plugin System)
  ├──> Phase 2 (Context Engine) ──┐
  ├──> Phase 3 (Frontend Plugins) ├──> Phase 4 (Core Features)
  ├──> Phase 6 (Provider Plugins) ├──> Phase 5 (Channel Plugins)
  └──────────────────────────────>└──> Phase 7 (Advanced Features)
```

Phases 2, 3, 6 can run **in parallel** after Phase 1.

---

## Core vs Plugin Decision Matrix

| Feature | Location | Rationale |
|---------|----------|-----------|
| Plugin system itself | Core (`crates/plugin`) | Foundation for everything |
| Context engine pipeline | Core (`crates/api`) | Affects every conversation |
| Auto-compaction | Core | Essential for long conversations |
| Chat commands | Core | Universal UX feature |
| Loop detection | Core | Safety feature |
| Retry/failover | Core | Reliability |
| Hybrid memory search | Core | Enhances existing KB system |
| Diff/patch tools | Core | Universal code editing |
| Link understanding | Core | Universal web tool |
| Reactions/polls | Core | Chat primitives |
| Enhanced scheduling | Core | Extends existing scheduler |
| Multi-agent routing | Core | Extends existing sub-agents |
| Channel integrations | **Plugin** | Each has unique deps, optional |
| LLM providers beyond 4 | **Plugin** | Many providers, each optional |
| Search providers beyond 2 | **Plugin** | Optional integrations |
| MCP bridge | **Plugin** | Protocol bridge, not core |
| Image generation | **Plugin** | Specialized capability |
| Voice/speech | **Plugin** | Complex, platform-specific |
| Canvas/A2UI | **Plugin** | Specialized UI, not core chat |
| Node system | **Plugin** | Device-specific, security-sensitive |

---

## Verification

After each phase, verify:

1. **Phase 1**: `cargo build --all-features` compiles. Plugin loads, registers a dummy tool, tool appears in agent's tool list. Plugin route accessible. Plugin migration runs on startup.
2. **Phase 2**: Long conversation auto-compacts. `/compact` command works. `/usage` returns token counts. Loop detection stops runaway tool calls. Retry works on simulated 429.
3. **Phase 3**: Plugin page appears in sidebar and routes. Plugin settings panel renders in Settings. Plugin connector card renders in Connectors.
4. **Phase 4**: `memory_write` tool persists to MEMORY.md entry. Hybrid search returns BM25+vector results. `apply_diff` modifies workspace files. Reactions render in chat. Polls are votable.
5. **Phase 5**: Telegram works via plugin (not core). Discord bot connects, receives/sends messages. At least 3 channel plugins pass end-to-end message flow test.
6. **Phase 6**: OpenAI-compat factory works for Groq/Together. MCP bridge discovers and exposes tools from a test MCP server. Image generation returns an image.
7. **Phase 7**: Canvas page renders, agent creates elements. Browser automation navigates and screenshots with CDP.
