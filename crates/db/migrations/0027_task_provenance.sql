-- Add provenance columns to scheduled_tasks so we can track which agent/conversation created a task.
ALTER TABLE scheduled_tasks
  ADD COLUMN created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
  ADD COLUMN created_by_conversation_id UUID REFERENCES conversations(id) ON DELETE SET NULL;
