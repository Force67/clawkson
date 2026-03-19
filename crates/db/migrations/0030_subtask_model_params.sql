-- Separate temperature and max-tokens configuration for sub-task execution.
-- When set, delegate_tasks uses these instead of the agent's primary values.
-- Allows tuning sub-agent behavior independently (e.g. lower temp for focused subtasks).

ALTER TABLE agents
    ADD COLUMN subtask_temperature DOUBLE PRECISION,
    ADD COLUMN subtask_max_tokens INTEGER;
