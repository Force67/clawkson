-- Add metadata JSONB column to chat_attachments for storing extracted
-- information about Office files (sheet names, slide counts, etc.)
ALTER TABLE chat_attachments
    ADD COLUMN IF NOT EXISTS metadata JSONB;
