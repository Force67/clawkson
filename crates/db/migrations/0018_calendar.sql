-- Calendar events with per-user ownership
CREATE TABLE calendar_events (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title        TEXT NOT NULL,
    date         DATE NOT NULL,
    start_time   TIME NOT NULL DEFAULT '09:00',
    end_time     TIME NOT NULL DEFAULT '10:00',
    category     TEXT NOT NULL DEFAULT 'work',
    location     TEXT,
    notes        TEXT,
    completed    BOOLEAN NOT NULL DEFAULT FALSE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_calendar_events_owner ON calendar_events(owner_id);
CREATE INDEX idx_calendar_events_date ON calendar_events(date);
CREATE INDEX idx_calendar_events_owner_date ON calendar_events(owner_id, date);

-- Calendar-level sharing: share your entire calendar with another user.
-- Reuses the existing share_permission enum (read / write).
CREATE TABLE calendar_shares (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id        UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    shared_with     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permission      share_permission NOT NULL DEFAULT 'read',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (owner_id, shared_with)
);
