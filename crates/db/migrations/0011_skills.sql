-- Skills: reusable prompt modules that agents can invoke with /skill-name syntax
CREATE TABLE skills (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT        NOT NULL UNIQUE,
    description     TEXT        NOT NULL DEFAULT '',
    instructions    TEXT        NOT NULL DEFAULT '',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Junction table: which skills are assigned to which agents
CREATE TABLE agent_skills (
    agent_id    UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    skill_id    UUID NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
    PRIMARY KEY (agent_id, skill_id)
);

CREATE INDEX idx_agent_skills_agent ON agent_skills (agent_id);
CREATE INDEX idx_agent_skills_skill ON agent_skills (skill_id);
