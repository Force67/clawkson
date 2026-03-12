-- Add ownership and sharing to agents
ALTER TABLE agents
    ADD COLUMN owner_id UUID REFERENCES users (id) ON DELETE SET NULL,
    ADD COLUMN shared   BOOLEAN NOT NULL DEFAULT FALSE;

CREATE INDEX idx_agents_owner ON agents (owner_id);
