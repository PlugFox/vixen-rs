-- M3 — Reports.
--
-- Three changes:
--
-- 1. Per-chat config gets four new tunables that the M3 report pipeline reads:
--      * report_min_activity — daily-report scheduler skips chats with fewer
--        messages_seen than this (avoids posting empty reports in dead chats).
--      * openai_api_key      — per-chat OpenAI key. Summary feature is enabled
--        chat-by-chat: a chat gets AI summaries iff this is set AND
--        summary_enabled = TRUE. NULL means "no summary for this chat" and is
--        the default.
--      * openai_model        — per-chat model selection (default gpt-4o-mini).
--      * language            — per-chat report language (default 'ru'). Drives
--        which locale strings the renderer emits. Single column over a JSONB
--        because we only need one short value.
--
-- 2. report_messages is recreated. The M0 schema kept a single message_id per
--    (chat_id, kind), which can't represent a daily report's two messages
--    (text + chart photo) at once and can't support replace-on-redo. The new
--    schema keys on (chat_id, report_date, kind) where kind discriminates
--    'daily_text' / 'daily_photo'. report_date carries the chat-local date
--    the report was generated for, so re-running on the same day finds the
--    prior pair and can delete-then-INSERT idempotently.
--
-- 3. spam_messages_per_chat — new chat-scoped counter table. The global
--    spam_messages stays as the canonical sample-body store keyed on the
--    xxh3-64 hash, but the per-chat hit counter that drives the daily-report
--    "top phrases" section moves into a dedicated table so reports for chat A
--    never surface phrases that only fired in chat B.

BEGIN;

-- ── chat_config ────────────────────────────────────────────────────────
ALTER TABLE chat_config
    ADD COLUMN report_min_activity SMALLINT     NOT NULL DEFAULT 20
        CHECK (report_min_activity >= 0),
    ADD COLUMN openai_api_key      TEXT,
    ADD COLUMN openai_model        VARCHAR(64)  NOT NULL DEFAULT 'gpt-4o-mini',
    ADD COLUMN language            VARCHAR(8)   NOT NULL DEFAULT 'ru'
        CHECK (language IN ('ru', 'en'));

-- ── report_messages ────────────────────────────────────────────────────
-- Drop the M0 placeholder shape and rebuild around (chat_id, report_date, kind).
-- The table is empty in M2 so DROP/CREATE is safe.
DROP TABLE report_messages;

CREATE TABLE report_messages (
    chat_id             BIGINT      NOT NULL REFERENCES chats(chat_id) ON DELETE CASCADE,
    report_date         DATE        NOT NULL,
    kind                TEXT        NOT NULL CHECK (kind IN ('daily_text', 'daily_photo')),
    telegram_message_id INTEGER     NOT NULL,
    generated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (chat_id, report_date, kind)
);
CREATE INDEX idx_report_messages_chat_date ON report_messages (chat_id, report_date DESC);

-- ── spam_messages_per_chat ─────────────────────────────────────────────
-- Per-chat counter for the global xxh3 dedup table. spam_messages.hit_count /
-- last_seen stay as the corpus-wide aggregate (drives the spam_cleanup TTL);
-- the per-chat row drives the report's "top phrases" section so phrases never
-- leak across chats. Both rows are bumped from the same write path; the FK on
-- xxh3_hash keeps them in lockstep so cleanup-by-hash cascades into the
-- per-chat counters.
CREATE TABLE spam_messages_per_chat (
    chat_id    BIGINT      NOT NULL REFERENCES chats(chat_id) ON DELETE CASCADE,
    xxh3_hash  BIGINT      NOT NULL REFERENCES spam_messages(xxh3_hash) ON DELETE CASCADE,
    hit_count  BIGINT      NOT NULL DEFAULT 1,
    last_seen  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (chat_id, xxh3_hash)
);
CREATE INDEX idx_spam_messages_per_chat_lookup
    ON spam_messages_per_chat (chat_id, last_seen DESC);

COMMIT;
