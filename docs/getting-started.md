# Getting Started

## Prerequisites

- **Rust** (stable, 2021 edition)
- **Bun** (v1.0+)
- **Docker** (optional, for agent containers)

## Setup

### Backend

```bash
# From the project root
cargo build
cargo run -p clawkson-api
# API will be available at http://localhost:47821
```

### Frontend

```bash
cd apps/web
bun install
bun run dev
# Dev server at http://localhost:47822
```

### Full Stack (Development)

Run the backend and frontend in separate terminals:

```bash
# Terminal 1 — Backend
cargo run -p clawkson-api

# Terminal 2 — Frontend
cd apps/web && bun run dev
```

### VectorChord (Optional)

Clawkson includes a local VectorChord Docker Compose setup for vector search experiments and future knowledge storage work.

```bash
cp .env.vectorchord.example .env
docker compose up -d vectorchord
cargo run -p clawkson-db
# PostgreSQL + VectorChord will be available at 127.0.0.1:55435 by default
```

See [VectorChord](./vectorchord.md) for environment variables, credentials, and first-boot behavior.

## Project Structure

```
clawkson/
├── AGENTS.md              # Project specification
├── openapi.yml            # API contract (source of truth)
├── docs/                  # Living documentation
├── apps/
│   └── web/               # React frontend (Bun + Vite)
│       └── src/
│           ├── components/ # Reusable UI components
│           └── pages/      # Page components (one per route)
└── crates/
    ├── clawkson-core/      # Domain models
    ├── clawkson-db/        # Database bootstrap + SQLx migrations
    └── clawkson-api/       # HTTP API server
```

## Configuration

LLM connectors and other settings are configured through the **Settings** page in the web UI, or will be configurable via the API.
