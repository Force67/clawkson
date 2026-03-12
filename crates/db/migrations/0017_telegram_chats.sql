-- Map Telegram chat IDs to Clawkson conversations.
-- Each (connector_id, telegram_chat_id) pair gets its own conversation,
-- so different users talking to the same bot get separate threads.
CREATE TABLE telegram_chats (
    connector_id    UUID        NOT NULL REFERENCES connectors(id) ON DELETE CASCADE,
    telegram_chat_id BIGINT     NOT NULL,
    conversation_id  UUID       NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    telegram_username TEXT,
    telegram_first_name TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (connector_id, telegram_chat_id)
);

CREATE INDEX idx_telegram_chats_conversation ON telegram_chats(conversation_id);
