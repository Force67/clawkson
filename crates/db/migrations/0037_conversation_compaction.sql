-- Track compaction state per conversation.
ALTER TABLE conversations
    ADD COLUMN IF NOT EXISTS compaction_summary TEXT,
    ADD COLUMN IF NOT EXISTS compacted_at TIMESTAMPTZ;
