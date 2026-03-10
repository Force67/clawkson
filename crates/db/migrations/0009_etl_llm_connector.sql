-- Add ETL LLM connector for semantic chunking during Knowledge Base ingestion
ALTER TABLE app_settings
    ADD COLUMN etl_llm_connector_id UUID REFERENCES llm_connectors(id) ON DELETE SET NULL;
