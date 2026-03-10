-- Platform connector type enum
CREATE TYPE connector_type AS ENUM ('telegram', 'gmail', 'slack', 'azure_devops', 'custom');

-- User-scoped platform connectors
CREATE TABLE connectors (
    id              UUID            PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID            NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name            TEXT            NOT NULL,
    connector_type  connector_type  NOT NULL,
    enabled         BOOLEAN         NOT NULL DEFAULT TRUE,
    config          JSONB           NOT NULL DEFAULT '{}',
    created_at      TIMESTAMPTZ     NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ     NOT NULL DEFAULT now()
);

CREATE INDEX idx_connectors_user ON connectors (user_id);
CREATE INDEX idx_connectors_type ON connectors (connector_type);
