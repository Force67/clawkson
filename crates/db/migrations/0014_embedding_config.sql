-- Configurable embedding provider (OpenAI API-compatible)
ALTER TABLE app_settings
    ADD COLUMN embedding_api_base_url TEXT NOT NULL DEFAULT 'http://localhost:11434/v1',
    ADD COLUMN embedding_api_key      Text NOT NULL DEFAULT 'ollama',
    ADD COLUMN embedding_model        TEXT NOT NULL DEFAULT 'qwen3-embedding:8b';
