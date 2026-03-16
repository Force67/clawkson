-- Migration: user profile fields + connector context
-- Adds bio (free-text agent context about the user) and avatar_url to users.
-- Adds context (free-text operational context) to connectors.

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS bio        TEXT    NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS avatar_url TEXT    NOT NULL DEFAULT '';

ALTER TABLE connectors
    ADD COLUMN IF NOT EXISTS context TEXT NOT NULL DEFAULT '';
