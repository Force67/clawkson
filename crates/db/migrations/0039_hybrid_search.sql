-- Add tsvector column for BM25 full-text search alongside vector similarity.
-- This enables hybrid search combining BM25 + vector via reciprocal rank fusion.

ALTER TABLE knowledge_entries
    ADD COLUMN IF NOT EXISTS tsv tsvector;

-- Auto-populate tsvector on insert/update.
CREATE OR REPLACE FUNCTION knowledge_entries_tsv_trigger() RETURNS trigger AS $$
BEGIN
  NEW.tsv := to_tsvector('english', COALESCE(NEW.content, ''));
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS tsvector_update ON knowledge_entries;
CREATE TRIGGER tsvector_update
  BEFORE INSERT OR UPDATE OF content ON knowledge_entries
  FOR EACH ROW EXECUTE FUNCTION knowledge_entries_tsv_trigger();

-- Backfill existing rows.
UPDATE knowledge_entries SET tsv = to_tsvector('english', COALESCE(content, ''))
WHERE tsv IS NULL;

-- GIN index for fast full-text search.
CREATE INDEX IF NOT EXISTS idx_knowledge_entries_tsv ON knowledge_entries USING GIN (tsv);
