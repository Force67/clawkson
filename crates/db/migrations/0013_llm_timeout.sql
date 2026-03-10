-- Add configurable LLM request timeout (seconds).
-- Default 120s gives slow providers (Azure, heavy models) time to respond.
ALTER TABLE app_settings
    ADD COLUMN llm_request_timeout_secs INTEGER NOT NULL DEFAULT 120;
