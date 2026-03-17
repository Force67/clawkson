-- Optional LLM connector for sub-task execution.
-- When set, sub-agents spawned by delegate_tasks use this connector instead of the agent's primary one.
-- Useful for routing sub-tasks to a cheaper/faster model.
ALTER TABLE agents ADD COLUMN subtask_llm_connector_id UUID REFERENCES llm_connectors(id) ON DELETE SET NULL;
