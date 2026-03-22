-- Enhanced scheduling: one-shot timers, intervals, heartbeat, standing orders.

ALTER TABLE scheduled_tasks
    ADD COLUMN IF NOT EXISTS task_type TEXT NOT NULL DEFAULT 'cron',
    ADD COLUMN IF NOT EXISTS interval_secs INTEGER,
    ADD COLUMN IF NOT EXISTS heartbeat_idle_secs INTEGER,
    ADD COLUMN IF NOT EXISTS last_user_activity_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS standing_order BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS run_once_at TIMESTAMPTZ;

-- task_type values: 'cron', 'interval', 'once', 'heartbeat', 'standing_order'
COMMENT ON COLUMN scheduled_tasks.task_type IS 'cron|interval|once|heartbeat|standing_order';
COMMENT ON COLUMN scheduled_tasks.interval_secs IS 'For interval type: seconds between runs';
COMMENT ON COLUMN scheduled_tasks.heartbeat_idle_secs IS 'For heartbeat type: run if no user activity for N seconds';
COMMENT ON COLUMN scheduled_tasks.run_once_at IS 'For once type: specific timestamp to run at';
