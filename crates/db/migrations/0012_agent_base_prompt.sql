-- Add a platform-level base system prompt to app_settings.
-- This prompt is prepended before every agent's user-defined system_prompt,
-- allowing admins to set global steering instructions (guardrails, tool usage
-- rules, identity, container permissions, etc.) that apply to ALL agents.
ALTER TABLE app_settings
    ADD COLUMN agent_base_prompt TEXT NOT NULL DEFAULT '';
