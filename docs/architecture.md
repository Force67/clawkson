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
| `/conversations` | Conversations | Chat interface with agents |
| `/knowledge` | Knowledge Base | Manage shared knowledge entries |
| `/connectors` | Connectors | Platform integrations (Telegram, Gmail, etc.) |
| `/tools` | Tools | Tools provided by connectors, `@toolname` invocation |
| `/settings` | Settings | LLM config, appearance, general settings |
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
| GET | `/agents/{id}` | Get agent by ID |
| GET/POST | `/conversations` | List/create conversations |
| GET | `/conversations/{id}` | Get conversation by ID |
| GET/POST | `/conversations/{id}/messages` | List/send messages |
| GET/POST | `/connectors` | List/create connectors |
| GET | `/connectors/{id}` | Get connector by ID |
| GET/POST | `/knowledge` | List/create knowledge entries |
| GET | `/knowledge/{id}` | Get knowledge entry by ID |
| GET/POST | `/tools` | List/create tools |
| GET | `/tools/{id}` | Get tool by ID |

## Orchestration

Multi-agent orchestration is handled by [Denkwerk](https://github.com/Force67/denkwerk). Agents run in isolated Docker containers for security.

## API Specification

The canonical API contract is maintained in `openapi.yml` at the project root. Both the frontend and backend must conform to this spec.
