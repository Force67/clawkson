-- LLM provider type enum
CREATE TYPE llm_provider_type AS ENUM ('azure', 'openrouter', 'openai', 'custom');

-- LLM connector configurations (provider keys, endpoints, models)
CREATE TABLE llm_connectors (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT NOT NULL,
    provider_type   llm_provider_type NOT NULL,
    api_key         TEXT NOT NULL DEFAULT '',
    api_base_url    TEXT NOT NULL DEFAULT '',
    model           TEXT NOT NULL DEFAULT '',
    azure_deployment   TEXT,
    azure_api_version  TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Application-wide settings (singleton row)
CREATE TABLE app_settings (
    id                      INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    default_llm_connector_id UUID REFERENCES llm_connectors(id) ON DELETE SET NULL,
    theme                   TEXT NOT NULL DEFAULT 'dark',
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Seed the single settings row
INSERT INTO app_settings (id) VALUES (1);
