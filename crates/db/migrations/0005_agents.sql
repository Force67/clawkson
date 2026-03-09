-- Agent status enum
CREATE TYPE agent_status AS ENUM ('online', 'offline', 'busy', 'error');

-- Agents table
CREATE TABLE agents (
    id                  UUID          PRIMARY KEY DEFAULT gen_random_uuid(),
    name                TEXT          NOT NULL,
    description         TEXT          NOT NULL DEFAULT '',
    status              agent_status  NOT NULL DEFAULT 'offline',
    llm_connector_id    UUID,
    system_prompt       TEXT,
    temperature         DOUBLE PRECISION,
    max_tokens          INTEGER,
    container_enabled   BOOLEAN       NOT NULL DEFAULT FALSE,
    container_config    JSONB,
    created_at          TIMESTAMPTZ   NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ   NOT NULL DEFAULT now()
);

-- Link existing conversations.agent_id to the new agents table
-- (not a FK since agent_id was already nullable and may have stale data)
CREATE INDEX idx_agents_status ON agents (status);
