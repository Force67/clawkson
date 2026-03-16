-- Switch from qwen3-embedding:8b (4096-dim) to qwen3-embedding:4b (2560-dim)

-- Drop the vector index first
DROP INDEX IF EXISTS idx_ke_embedding;

-- Clear existing embeddings (wrong dimension, must be re-embedded)
UPDATE knowledge_entries SET embedding = NULL;

-- Change column from vector(4096) to vector(2560)
ALTER TABLE knowledge_entries
    ALTER COLUMN embedding TYPE vector(2560);

-- Recreate the VectorChord index
CREATE INDEX idx_ke_embedding ON knowledge_entries
    USING vchordrq (embedding vector_cosine_ops);

-- Update default embedding model references
ALTER TABLE knowledge_bases
    ALTER COLUMN embedding_model SET DEFAULT 'qwen3-embedding:4b';

ALTER TABLE app_settings
    ALTER COLUMN embedding_model SET DEFAULT 'qwen3-embedding:4b';

-- Update existing rows that still reference the old model
UPDATE knowledge_bases SET embedding_model = 'qwen3-embedding:4b'
    WHERE embedding_model = 'qwen3-embedding:8b';

UPDATE app_settings SET embedding_model = 'qwen3-embedding:4b'
    WHERE embedding_model = 'qwen3-embedding:8b';
