CREATE TABLE knowledge_documents (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    knowledge_base_id UUID NOT NULL REFERENCES knowledge_bases(id) ON DELETE CASCADE,
    filename          TEXT NOT NULL,
    content_type      TEXT NOT NULL DEFAULT 'application/octet-stream',
    s3_key            TEXT NOT NULL,
    size_bytes        BIGINT NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_kd_kb ON knowledge_documents(knowledge_base_id);

ALTER TABLE knowledge_entries
    ADD COLUMN source_document_id UUID REFERENCES knowledge_documents(id) ON DELETE SET NULL;
CREATE INDEX idx_ke_source_doc ON knowledge_entries(source_document_id);
