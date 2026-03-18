-- Per-user LLM connector access control
-- When shared_with_all = true (default), all users can use the connector.
-- When false, only users with a row in user_llm_access can use it.
ALTER TABLE llm_connectors ADD COLUMN shared_with_all BOOLEAN NOT NULL DEFAULT TRUE;

CREATE TABLE user_llm_access (
    connector_id UUID NOT NULL REFERENCES llm_connectors(id) ON DELETE CASCADE,
    user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    granted_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (connector_id, user_id)
);

CREATE INDEX idx_user_llm_access_user ON user_llm_access(user_id);

-- Token usage tracking (one row per LLM call)
CREATE TABLE token_usage (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id           UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    connector_id      UUID REFERENCES llm_connectors(id) ON DELETE SET NULL,
    model             TEXT NOT NULL,
    prompt_tokens     INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens      INTEGER NOT NULL DEFAULT 0,
    conversation_id   UUID REFERENCES conversations(id) ON DELETE SET NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_token_usage_user      ON token_usage(user_id, created_at DESC);
CREATE INDEX idx_token_usage_connector ON token_usage(connector_id, created_at DESC);
CREATE INDEX idx_token_usage_model     ON token_usage(model, created_at DESC);
