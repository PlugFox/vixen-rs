-- Reverts 20260503000000_m3_reports.up.sql.
--
-- report_messages reverts to the M0 placeholder shape. Any rows that the M3
-- daily-report job wrote are lost — that is the same data-loss tradeoff the
-- forward migration's DROP TABLE made when the table was empty.

BEGIN;

DROP TABLE spam_messages_per_chat;

DROP TABLE report_messages;

CREATE TABLE report_messages (
    chat_id             BIGINT      NOT NULL REFERENCES chats(chat_id) ON DELETE CASCADE,
    kind                TEXT        NOT NULL CHECK (kind IN ('daily')),
    telegram_message_id BIGINT      NOT NULL,
    generated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (chat_id, kind)
);

ALTER TABLE chat_config
    DROP COLUMN language,
    DROP COLUMN openai_model,
    DROP COLUMN openai_api_key,
    DROP COLUMN report_min_activity;

COMMIT;
