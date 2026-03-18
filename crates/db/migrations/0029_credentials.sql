-- Credentials store: named secrets that agents can reference by name.
-- Actual values are never exposed to the LLM context; they are resolved
-- at the tool execution layer only.

CREATE TABLE credentials (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id        UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name            TEXT        NOT NULL,
    description     TEXT        NOT NULL DEFAULT '',
    credential_type TEXT        NOT NULL DEFAULT 'api_key',  -- api_key, password, token, secret, header
    encrypted_value TEXT        NOT NULL,  -- plaintext MVP, encrypted at-rest later
    metadata        JSONB       NOT NULL DEFAULT '{}',       -- extra fields (e.g. header_name for type=header)
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(owner_id, name)
);

CREATE TABLE agent_credentials (
    agent_id      UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    credential_id UUID NOT NULL REFERENCES credentials(id) ON DELETE CASCADE,
    PRIMARY KEY (agent_id, credential_id)
);

CREATE INDEX idx_credentials_owner ON credentials (owner_id);
CREATE INDEX idx_agent_credentials_agent ON agent_credentials (agent_id);
CREATE INDEX idx_agent_credentials_credential ON agent_credentials (credential_id);
