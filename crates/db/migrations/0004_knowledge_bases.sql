-- Knowledge Bases (user-scoped, shareable containers for entries)
CREATE TABLE knowledge_bases (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id    UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name        TEXT        NOT NULL,
    description TEXT        NOT NULL DEFAULT '',
    embedding_model TEXT    NOT NULL DEFAULT 'qwen3-embedding:8b',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_kb_owner ON knowledge_bases (owner_id);

-- Knowledge entries with vector embeddings (4096 dim for qwen3-embedding)
CREATE TABLE knowledge_entries (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    knowledge_base_id UUID     NOT NULL REFERENCES knowledge_bases (id) ON DELETE CASCADE,
    title           TEXT        NOT NULL,
    content         TEXT        NOT NULL,
    token_count     INTEGER,
    embedding       vector(4096),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_ke_kb ON knowledge_entries (knowledge_base_id);

-- VectorChord index for fast similarity search
CREATE INDEX idx_ke_embedding ON knowledge_entries
    USING vchordrq (embedding vector_cosine_ops);

-- Knowledge base sharing (same pattern as conversation shares)
CREATE TABLE knowledge_base_shares (
    id                UUID              PRIMARY KEY DEFAULT gen_random_uuid(),
    knowledge_base_id UUID              NOT NULL REFERENCES knowledge_bases (id) ON DELETE CASCADE,
    shared_by         UUID              NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    shared_with       UUID              NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    permission        share_permission  NOT NULL DEFAULT 'read',
    created_at        TIMESTAMPTZ       NOT NULL DEFAULT now(),
    UNIQUE (knowledge_base_id, shared_with)
);

CREATE INDEX idx_kbs_kb ON knowledge_base_shares (knowledge_base_id);
CREATE INDEX idx_kbs_shared_with ON knowledge_base_shares (shared_with);

-- Agent ↔ Knowledge Base access (many-to-many)
CREATE TABLE agent_knowledge_bases (
    agent_id          UUID NOT NULL,
    knowledge_base_id UUID NOT NULL REFERENCES knowledge_bases (id) ON DELETE CASCADE,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (agent_id, knowledge_base_id)
);

CREATE INDEX idx_akb_agent ON agent_knowledge_bases (agent_id);
CREATE INDEX idx_akb_kb ON agent_knowledge_bases (knowledge_base_id);
