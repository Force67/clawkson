# VectorChord

Clawkson ships with a Docker Compose setup for a local VectorChord-backed PostgreSQL instance.

## Upstream Version

The compose file is pinned to `ghcr.io/tensorchord/vchord-postgres:pg18-v1.1.1`.

This matches the latest upstream VectorChord release at the time of writing:
- **VectorChord:** `1.1.1`
- **Published:** February 28, 2026

## Why Port `55435`

This machine already has listeners on `5432`, `5433`, `5434`, and `55432`, so Clawkson binds VectorChord to `127.0.0.1:55435` by default to avoid collisions.

The port can be overridden with `CLAWKSON_VECTORCHORD_PORT`.

## Files

- [`docker-compose.yml`](../docker-compose.yml) — VectorChord PostgreSQL service
- [`crates/db/Cargo.toml`](../crates/db/Cargo.toml) — `clawkson-db` migration crate
- [`crates/db/migrations/0001_init.sql`](../crates/db/migrations/0001_init.sql) — initial VectorChord migration
- [`.env.vectorchord.example`](../.env.vectorchord.example) — shared environment template

## Startup

```bash
cp .env.vectorchord.example .env
docker compose up -d vectorchord
cargo run -p clawkson-db
```

The container:
- exposes PostgreSQL only on localhost
- persists data in the `vectorchord-data` volume

The `clawkson-db` crate then:
- creates or updates the dedicated `clawkson` role
- creates the `clawkson` database if it does not exist
- runs SQLx migrations against that database
- enables `vchord` with its required dependencies via `CASCADE`

## Connection Details

Default DSN:

```text
postgresql://clawkson:change-me-clawkson-password@127.0.0.1:55435/clawkson
```

Admin connection:

```text
postgresql://postgres:change-me-superuser-password@127.0.0.1:55435/postgres
```

## Notes

- Change both default passwords before using this outside a local development setup.
- `cargo run -p clawkson-db` loads `.env` automatically and is safe to rerun.
- If credentials or ownership change after the database already exists, rerun `clawkson-db` to reconcile the role password and database bootstrap state.
- `CREATE EXTENSION IF NOT EXISTS vchord CASCADE;` also installs `pgvector` as required by VectorChord.
