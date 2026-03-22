-- Model pricing for cost estimation
CREATE TABLE model_pricing (
    id                          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    model                       TEXT NOT NULL UNIQUE,
    prompt_cost_per_million      NUMERIC(12, 6) NOT NULL DEFAULT 0,
    completion_cost_per_million  NUMERIC(12, 6) NOT NULL DEFAULT 0,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Speed up per-conversation usage lookups
CREATE INDEX idx_token_usage_conversation ON token_usage(conversation_id) WHERE conversation_id IS NOT NULL;
