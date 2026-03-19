-- Per-agent knowledge memory: each agent gets its own memory KB
-- instead of one shared per-user memory.

-- Add agent_id column to knowledge_bases
ALTER TABLE knowledge_bases
  ADD COLUMN agent_id UUID REFERENCES agents(id) ON DELETE SET NULL;

CREATE INDEX idx_kb_agent ON knowledge_bases (agent_id);

-- Drop old per-user memory unique index
DROP INDEX IF EXISTS idx_kb_memory_per_user;

-- Create per-agent memory unique index (one memory KB per agent)
CREATE UNIQUE INDEX idx_kb_memory_per_agent
  ON knowledge_bases (agent_id) WHERE kb_type = 'memory';

-- Convert existing user memory KBs to standard (legacy, user can review/delete)
UPDATE knowledge_bases
  SET name = 'Legacy Memory', kb_type = 'standard'
  WHERE kb_type = 'memory' AND agent_id IS NULL;
