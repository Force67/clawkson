-- User roles
CREATE TYPE user_role AS ENUM ('admin', 'user');

-- Users table
CREATE TABLE users (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    email       TEXT        NOT NULL UNIQUE,
    display_name TEXT       NOT NULL DEFAULT '',
    password_hash TEXT      NOT NULL,
    role        user_role   NOT NULL DEFAULT 'user',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX idx_users_email ON users (lower(email));

-- Sessions table (cookie-based auth)
CREATE TABLE sessions (
    token       TEXT        PRIMARY KEY,
    user_id     UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    expires_at  TIMESTAMPTZ NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_sessions_user ON sessions (user_id);
CREATE INDEX idx_sessions_expires ON sessions (expires_at);

-- Conversation sharing
CREATE TYPE share_permission AS ENUM ('read', 'write');

CREATE TABLE conversation_shares (
    id              UUID              PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id UUID              NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    shared_by       UUID              NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    shared_with     UUID              NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    permission      share_permission  NOT NULL DEFAULT 'read',
    created_at      TIMESTAMPTZ       NOT NULL DEFAULT now(),
    UNIQUE (conversation_id, shared_with)
);

CREATE INDEX idx_shares_conversation ON conversation_shares (conversation_id);
CREATE INDEX idx_shares_shared_with ON conversation_shares (shared_with);

-- Add owner_id to conversations (nullable for backwards compat during migration)
ALTER TABLE conversations ADD COLUMN owner_id UUID REFERENCES users (id) ON DELETE CASCADE;
CREATE INDEX idx_conversations_owner ON conversations (owner_id);
