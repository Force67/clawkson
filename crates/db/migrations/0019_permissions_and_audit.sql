-- Add connector policies (JSONB array of ConnectorPolicy) to agents.
-- Each entry defines allow/deny proxy rules for one connector.
ALTER TABLE agents
    ADD COLUMN connector_policies JSONB NOT NULL DEFAULT '[]';

-- Audit log for every tool invocation (allowed or denied).
-- This table is append-only and used for compliance, debugging, and analytics.
CREATE TABLE tool_audit_log (
    id                UUID          PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id   UUID          NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    agent_id          UUID          NOT NULL REFERENCES agents (id) ON DELETE CASCADE,
    user_id           UUID          NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    tool_name         TEXT          NOT NULL,
    http_method       TEXT,
    target_path       TEXT,
    connector_id      UUID          REFERENCES connectors (id) ON DELETE SET NULL,
    decision          TEXT          NOT NULL CHECK (decision IN ('allowed', 'denied')),
    denial_reason     TEXT,
    duration_ms       BIGINT,
    created_at        TIMESTAMPTZ   NOT NULL DEFAULT now()
);

-- Indexes for common query patterns
CREATE INDEX idx_audit_log_conversation ON tool_audit_log (conversation_id, created_at DESC);
CREATE INDEX idx_audit_log_agent        ON tool_audit_log (agent_id, created_at DESC);
CREATE INDEX idx_audit_log_user         ON tool_audit_log (user_id, created_at DESC);
CREATE INDEX idx_audit_log_decision     ON tool_audit_log (decision, created_at DESC);
CREATE INDEX idx_audit_log_connector    ON tool_audit_log (connector_id) WHERE connector_id IS NOT NULL;
