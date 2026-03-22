-- Track the active branch tip for conversation branching
ALTER TABLE conversations ADD COLUMN active_leaf_id UUID REFERENCES messages(id) ON DELETE SET NULL;
