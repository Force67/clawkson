-- Convert connector_type and llm_provider_type from PostgreSQL ENUMs to TEXT
-- This allows plugins to register new types without schema changes.

-- 1. connectors.connector_type: enum → TEXT
ALTER TABLE connectors
    ALTER COLUMN connector_type TYPE TEXT USING connector_type::TEXT;

DROP TYPE IF EXISTS connector_type;

-- 2. llm_connectors.provider_type: enum → TEXT
ALTER TABLE llm_connectors
    ALTER COLUMN provider_type TYPE TEXT USING provider_type::TEXT;

DROP TYPE IF EXISTS llm_provider_type;
