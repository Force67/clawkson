-- Add kb_type column to knowledge_bases for distinguishing standard vs memory KBs
ALTER TABLE knowledge_bases ADD COLUMN kb_type TEXT NOT NULL DEFAULT 'standard';

-- Index for filtering by kb_type
CREATE INDEX idx_kb_type ON knowledge_bases (kb_type);

-- Ensure each user can only have one memory knowledge base
CREATE UNIQUE INDEX idx_kb_memory_per_user ON knowledge_bases (owner_id) WHERE kb_type = 'memory';
