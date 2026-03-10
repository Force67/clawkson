# Getting Started

## Prerequisites

- **Rust** (stable, 2021 edition)
- **Bun** (v1.0+)
- **Docker** (for PostgreSQL + VectorChord + MinIO, and optional agent containers)

## Environment Setup

Copy the example env file and fill in your values:

```bash
cp .env.vectorchord.example .env
```

The `.env` file is loaded automatically at startup via `dotenvy`. The required and optional environment variables are:

### Database (required)

| Variable | Description |
|---|---|
| `DATABASE_URL` | PostgreSQL connection string (used by SQLx tooling) |
| `CLAWKSON_DB_HOST` | Postgres host (default: `localhost`) |
| `CLAWKSON_DB_PORT` | Postgres port (default: `55435`) |
| `CLAWKSON_DB_NAME` | Database name (default: `clawkson`) |
| `CLAWKSON_DB_ADMIN_USER` | Admin user for bootstrapping (default: `postgres`) |
| `CLAWKSON_DB_ADMIN_PASSWORD` | Admin password |
| `CLAWKSON_DB_USER` | App user (default: `clawkson_app`) |
| `CLAWKSON_DB_PASSWORD` | App user password |

### S3-compatible storage (optional — required for Knowledge Base document upload)

| Variable | Default | Description |
|---|---|---|
| `CLAWKSON_S3_ENDPOINT` | `http://localhost:9100` | MinIO / S3-compatible endpoint |
| `CLAWKSON_S3_ACCESS_KEY` | `clawkson` | Access key |
| `CLAWKSON_S3_SECRET_KEY` | `clawkson-secret-key` | Secret key |
| `CLAWKSON_S3_BUCKET` | `clawkson-documents` | Bucket name (auto-created if absent) |

### Frontend CORS (optional)

| Variable | Default | Description |
|---|---|---|
| `FRONTEND_ORIGIN` | `http://localhost:5173` | Allowed CORS origin for the frontend dev server |

### Container manager (optional)

| Variable | Default | Description |
|---|---|---|
| `CLAWKSON_WORKSPACE_ROOT` | `/tmp/clawkson-workspaces` | Host path for agent Docker workspace bind-mounts |

## Setup

### 1. Start infrastructure (PostgreSQL + VectorChord + MinIO)

```bash
docker compose up -d
# PostgreSQL + VectorChord on 127.0.0.1:55435
# MinIO on 127.0.0.1:9100
```

### 2. Backend

The binary crate is `clawkson-server`. The server bootstraps the database and runs migrations automatically on startup.

```bash
# From the project root
cargo run -p clawkson-server
# API available at http://localhost:47821
```

### 3. Frontend

```bash
cd apps/web
bun install
bun run dev
# Dev server at http://localhost:5173
```

### Full Stack (Development)

Run the backend and frontend in separate terminals:

```bash
# Terminal 1 — infrastructure
docker compose up -d

# Terminal 2 — backend
cargo run -p clawkson-server

# Terminal 3 — frontend
cd apps/web && bun run dev
```

## First Run

On first startup the database schema is created and all migrations are applied automatically. The **first user to register** becomes the administrator.

Navigate to `http://localhost:5173` and use the Register page to create your admin account.

## Project Structure

```
clawkson/
├── AGENTS.md              # Project specification
├── openapi.yml            # API contract (source of truth)
├── docker-compose.yml     # VectorChord + MinIO services
├── docs/                  # Living documentation
├── apps/
│   └── web/               # React frontend (Bun + Vite)
│       └── src/
│           ├── components/ # Reusable UI components
│           └── pages/      # Page components (one per route)
└── crates/
    ├── clawkson-server/    # Binary entry point (HTTP server startup)
    ├── clawkson-api/       # HTTP route handlers and middleware
    ├── clawkson-core/      # Domain models (shared types)
    ├── clawkson-db/        # Database bootstrap, SQLx migrations
    └── clawkson-container/ # Docker container management for agents
```

## Configuration

LLM connectors (API keys, provider type, model) and application settings are managed through the **Settings** page in the web UI or via the `/api/llm-connectors` and `/api/settings` API endpoints.

See [Connectors](./connectors.md) for full details on LLM connector configuration.
