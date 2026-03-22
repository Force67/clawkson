-- Track per-plugin migration state so each plugin can manage its own schema.
CREATE TABLE IF NOT EXISTS plugin_migrations (
    id          SERIAL PRIMARY KEY,
    plugin_name TEXT NOT NULL,
    version     INTEGER NOT NULL,
    applied_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(plugin_name, version)
);

-- Plugin settings: per-plugin key-value JSON config.
CREATE TABLE IF NOT EXISTS plugin_settings (
    plugin_name TEXT NOT NULL,
    enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    config      JSONB NOT NULL DEFAULT '{}',
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (plugin_name)
);
