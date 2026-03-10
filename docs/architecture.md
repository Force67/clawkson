# Architecture

## Overview

Clawkson is a multi-agent AI assistant platform with a clean separation between frontend, backend, and orchestration layers.

```
┌─────────────┐     ┌──────────────────┐     ┌──────────────┐
│  React UI   │────▶│  Rust API (Axum)  │────▶│  Denkwerk    │
│  (Bun/Vite) │     │  Port 47821       │     │  Orchestrator│
└─────────────┘     └──────────────────┘     └──────────────┘
                            │                        │
               ┌────────────┼────────────┐    ┌──────┴───────┐
               │            │            │    │  Docker       │
         ┌─────▼──────┐ ┌───▼──────┐ ┌──▼──┐ │  Containers  │
         │ PostgreSQL  │ │VectorChord│ │MinIO│ └──────────────┘
         │ (metadata) │ │(embeddings)│ │(S3) │
         └────────────┘ └──────────┘ └─────┘
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
| `/conversations` | Conversations | Real-time chat interface with grouped thread list, immersive chat canvas, and SSE streaming |
| `/agents` | Agents | Create, configure, and manage agents |
| `/knowledge` | Knowledge Base | Manage knowledge bases, entries, and documents |
| `/connectors` | Connectors | Platform integrations (Telegram, Gmail, etc.) |
| `/tools` | Tools | Tools provided by connectors, `@toolname` invocation |
| `/settings` | Settings | LLM connector management, appearance, general settings |
| `/docs` | Documentation | Rendered documentation |

## Backend (`crates/`)

- **Language:** Rust
- **Framework:** Axum 0.8
- **Workspace crates:**
  - `clawkson-server` — Binary entry point; wires up the database, container manager, S3 client, CORS, and HTTP listener
  - `clawkson-api` — All HTTP route handlers, middleware (auth session extraction), embeddings, and S3 helpers
  - `clawkson-core` — Shared domain models (Agent, Conversation, Message, KnowledgeBase, User, etc.)
  - `clawkson-db` — Database bootstrap, SQLx connection helpers, and migration runner
  - `clawkson-container` — Docker container lifecycle management for agent isolation

### API Routes
All routes are prefixed with `/api/`:

| Method | Path | Description |
|---|---|---|
| POST | `/auth/register` | Register a new user (first user becomes admin) |
| POST | `/auth/login` | Log in; sets `clawkson_session` HttpOnly cookie |
| POST | `/auth/logout` | Log out; clears session cookie |
| GET | `/auth/me` | Get the currently authenticated user |
| GET | `/admin/users` | List all users (admin only) |
| PATCH | `/admin/users/{id}/role` | Change a user's role (admin only) |
| DELETE | `/admin/users/{id}` | Delete a user (admin only) |
| GET/POST | `/agents` | List/create agents |
| GET/PATCH/DELETE | `/agents/{id}` | Get/update/delete agent |
| POST | `/agents/{id}/start` | Start agent container |
| POST | `/agents/{id}/stop` | Stop agent container |
| GET | `/agents/{id}/logs` | Stream container logs |
| POST | `/agents/{id}/exec` | Execute a command in container |
| GET | `/agents/{id}/status` | Get container status |
| POST | `/agents/{id}/remove` | Remove container |
| GET/POST | `/conversations` | List/create conversations |
| GET/PATCH/DELETE | `/conversations/{id}` | Get/update/delete conversation |
| GET/POST | `/conversations/{id}/messages` | List/send raw messages |
| POST | `/conversations/{id}/chat` | Send message + get AI response (blocking) |
| POST | `/conversations/{id}/chat/stream` | Send message + stream AI response (SSE) |
| GET/POST | `/conversations/{id}/shares` | List/create conversation shares |
| DELETE | `/conversations/{cid}/shares/{uid}` | Remove a conversation share |
| GET/POST | `/llm-connectors` | List/create LLM connectors |
| GET/PATCH/DELETE | `/llm-connectors/{id}` | Get/update/delete LLM connector |
| POST | `/llm-connectors/test` | Test connector credentials without saving |
| GET/PATCH | `/settings` | Get/update application settings (PATCH is admin only) |
| GET/POST | `/knowledge` | List/create knowledge bases |
| GET/PATCH/DELETE | `/knowledge/{id}` | Get/update/delete knowledge base |
| GET/POST | `/knowledge/{id}/entries` | List/create knowledge entries |
| PATCH/DELETE | `/knowledge/{kb_id}/entries/{entry_id}` | Update/delete a knowledge entry |
| POST | `/knowledge/{id}/upload` | Upload files; auto-chunk and embed |
| POST | `/knowledge/{id}/embed` | (Re-)generate embeddings for all entries |
| POST | `/knowledge/{id}/search` | Semantic search within a knowledge base |
| GET | `/knowledge/{kb_id}/documents` | List uploaded source documents |
| GET | `/knowledge/{kb_id}/documents/{doc_id}/download` | Download a source document from S3 |
| DELETE | `/knowledge/{kb_id}/documents/{doc_id}` | Delete a source document |
| GET/POST | `/knowledge/{id}/shares` | List/create knowledge base shares |
| DELETE | `/knowledge/{kb_id}/shares/{user_id}` | Remove a knowledge base share |
| GET/POST | `/knowledge/{id}/agents` | List/link agents to a knowledge base |
| DELETE | `/knowledge/{kb_id}/agents/{agent_id}` | Unlink an agent from a knowledge base |
| GET/POST | `/uploads` | List/upload generic file attachments |
| GET/DELETE | `/uploads/{id}` | Get/delete a file attachment |
| GET/POST | `/connectors` | List/create platform connectors |
| GET/PATCH/DELETE | `/connectors/{id}` | Get/update/delete platform connector |
| GET/POST | `/tools` | List/create tools |
| GET | `/tools/{id}` | Get tool by ID |

## Data Layer

Clawkson persists all data to **PostgreSQL with the VectorChord extension** (`vchordrq` index type for cosine-distance vector search). The schema is managed by 8 SQLx migrations in `crates/db/migrations/`.

- **Image:** `ghcr.io/tensorchord/vchord-postgres:pg18-v1.1.1`
- **Default host binding:** `127.0.0.1:55435`
- **Bootstrap:** `clawkson-server` connects as the admin user, creates the app database and user if absent, runs all pending migrations, then reconnects as the app user

### Document Storage

Original uploaded files (PDF, TXT, MD, CSV, JSON) are stored in **MinIO** (S3-compatible). By default MinIO is expected at `http://localhost:9100`. The bucket (`clawkson-documents`) is auto-created on first connection.

### Knowledge Base pipeline

1. File uploaded via `POST /knowledge/{id}/upload`
2. Text extracted per file type (PDF via `pdf_extract`, others as plain text)
3. Content split into chunks of up to 4 000 characters with 200-character overlap:
   - **Heuristic (default):** paragraph → sentence → word boundary splitting
   - **Semantic (optional):** when an ETL LLM connector is configured in Settings, the LLM is called for each oversized chunk to identify the optimal sentence-boundary split position; only small context windows are sent to the LLM, never the full document
4. Each chunk stored as a `KnowledgeEntry` row
5. Embeddings generated in batches of 8 via Ollama (`http://localhost:11434/v1`); default model `qwen3-embedding:8b`; 4096-dimensional vectors indexed with `vchordrq` using cosine distance

#### ETL LLM Configuration

Go to **Settings → ETL Processing** and select any configured LLM connector as the *Semantic Chunking Model*. Setting this to "None" falls back to heuristic splitting. The ETL connector is independent of the agent default connector — you can use a cheap, fast model (e.g. a small local Ollama model) for chunking while routing agent conversations to a more capable model.

## Authentication

Sessions use **HttpOnly cookies** (`clawkson_session`). Passwords are hashed with **bcrypt**. The first user to register is automatically assigned the `admin` role.

## LLM Provider Layer

The `crates/api/src/llm.rs` module is a thin adapter over a local `denkwerk` dependency:
- `complete(connector, messages)` — blocking chat completion
- `stream_complete(connector, messages, callback)` — streaming via delta callback

Supported providers: **Azure OpenAI**, **OpenRouter**, **OpenAI**, **Custom (OpenAI-compatible)**.

LLM connector API keys are **encrypted and stored in the database** — they persist across server restarts.

## API Specification

The canonical API contract is maintained in `openapi.yml` at the project root. Both the frontend and backend must conform to this spec.
