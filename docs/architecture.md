# Architecture

## Overview

Clawkson is a multi-agent AI assistant platform with a clean separation between frontend, backend, and orchestration layers.

```
┌─────────────┐     ┌─────────────────┐     ┌──────────────┐
│  React UI   │────▶│  Rust API (Axum) │────▶│  Denkwerk    │
│  (Bun/Vite) │     │  Port 3001       │     │  Orchestrator│
└─────────────┘     └─────────────────┘     └──────────────┘
                            │                       │
                    ┌───────┴───────┐       ┌───────┴───────┐
                    │  In-Memory    │       │  Docker       │
                    │  State (MVP)  │       │  Containers   │
                    └───────────────┘       └───────────────┘
```

## Frontend (`apps/web/`)

- **Framework:** React 19 + TypeScript
- **Bundler:** Vite (via Bun)
- **Routing:** react-router-dom v7
- **Icons:** lucide-react
- **Styling:** CSS Modules with CSS custom properties (dark theme)

### Pages
| Route | Page | Description |
|---|---|---|
| `/dashboard` | Dashboard | Agent overview, stats, activity feed |
| `/conversations` | Conversations | Real-time chat interface with SSE streaming |
| `/agents` | Agents | Create, configure, and manage agents |
| `/knowledge` | Knowledge Base | Manage shared knowledge entries |
| `/connectors` | Connectors | Platform integrations (Telegram, Gmail, etc.) |
| `/tools` | Tools | Tools provided by connectors, `@toolname` invocation |
| `/settings` | Settings | LLM connector management, appearance, general settings |
| `/docs` | Documentation | Rendered documentation |

## Backend (`crates/`)

- **Language:** Rust
- **Framework:** Axum 0.8
- **Workspace crates:**
  - `clawkson-core` — Domain models (Agent, Conversation, Message, Connector, Tool, etc.)
  - `clawkson-api` — HTTP API server with routes for all resources

### API Routes
All routes are prefixed with `/api/`:

| Method | Path | Description |
|---|---|---|
| GET/POST | `/agents` | List/create agents |
| GET/PATCH/DELETE | `/agents/{id}` | Get/update/delete agent |
| GET/POST | `/conversations` | List/create conversations |
| GET | `/conversations/{id}` | Get conversation by ID |
| GET/POST | `/conversations/{id}/messages` | List/send raw messages |
| POST | `/conversations/{id}/chat` | Send message + get AI response (blocking) |
| POST | `/conversations/{id}/chat/stream` | Send message + stream AI response (SSE) |
| GET/POST | `/llm-connectors` | List/create LLM connectors |
| GET/PATCH/DELETE | `/llm-connectors/{id}` | Get/update/delete LLM connector |
| GET/PATCH | `/settings` | Get/update application settings |
| GET/POST | `/connectors` | List/create platform connectors |
| GET | `/connectors/{id}` | Get connector by ID |
| GET/POST | `/knowledge` | List/create knowledge entries |
| GET | `/knowledge/{id}` | Get knowledge entry by ID |
| GET/POST | `/tools` | List/create tools |
| GET | `/tools/{id}` | Get tool by ID |

## LLM Provider Layer

The `crates/api/src/llm.rs` module is a thin adapter over a local `denkwerk` dependency:
- `complete(connector, messages)` — blocking chat completion
- `stream_complete(connector, messages, callback)` — streaming via delta callback

Supported providers: **Azure OpenAI**, **OpenRouter**, **OpenAI**, **Custom (OpenAI-compatible)**.

Clawkson currently depends on a local Denkwerk checkout with the GUI/editor path feature-gated so the backend can use provider implementations without pulling `iced` into the API server build.

## API Specification

The canonical API contract is maintained in `openapi.yml` at the project root. Both the frontend and backend must conform to this spec.
