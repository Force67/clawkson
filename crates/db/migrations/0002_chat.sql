-- Enums
CREATE TYPE message_role AS ENUM ('user', 'assistant', 'system', 'tool');
CREATE TYPE message_status AS ENUM ('sending', 'sent', 'streaming', 'failed', 'cancelled');

-- Conversations
CREATE TABLE conversations (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id    UUID,
    title       TEXT NOT NULL DEFAULT '',
    summary     TEXT,
    pinned      BOOLEAN NOT NULL DEFAULT FALSE,
    archived    BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_conversations_agent_id ON conversations (agent_id);
CREATE INDEX idx_conversations_updated_at ON conversations (updated_at DESC);

-- Messages
CREATE TABLE messages (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id UUID NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    parent_id       UUID REFERENCES messages (id) ON DELETE SET NULL,
    role            message_role NOT NULL,
    content         TEXT NOT NULL DEFAULT '',
    model           TEXT,
    metadata        JSONB,
    token_count     INTEGER,
    status          message_status NOT NULL DEFAULT 'sent',
    edited          BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_messages_conversation ON messages (conversation_id, created_at);
CREATE INDEX idx_messages_parent ON messages (parent_id) WHERE parent_id IS NOT NULL;
