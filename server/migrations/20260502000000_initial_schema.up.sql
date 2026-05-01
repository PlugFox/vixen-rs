-- Initial schema for vixen-rs M0–M3.
--
-- Tables and conventions follow server/docs/database.md exactly. The eleven
-- tables cover every persistence need through M3: chats and per-chat config,
-- moderators, verified users, captcha challenges, spam-hash dedup, the action
-- audit ledger, daily report tracking, daily stat aggregates, the chat-info
-- cache, and the optional gated allowed-messages log.
--
-- Telegram IDs are BIGINT NOT NULL throughout (i64 in Rust).
-- xxh3-64 hashes are BIGINT (round-trip via cast_signed/cast_unsigned).
-- Foreign keys to chats(chat_id) cascade on delete — purging a watched chat
-- wipes its history.

BEGIN;

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ── Trigger function: maintain `updated_at` automatically ────────────────
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ── chats ────────────────────────────────────────────────────────────────
CREATE TABLE chats (
    chat_id    BIGINT      PRIMARY KEY,
    slug       VARCHAR(64) UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE TRIGGER trg_chats_updated_at
    BEFORE UPDATE ON chats
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ── chat_config ──────────────────────────────────────────────────────────
-- One row per watched chat. Source of truth for per-chat tunables; edited from
-- the dashboard, hot-reloaded via Redis pub/sub on `chat_config:{chat_id}`.
CREATE TABLE chat_config (
    chat_id               BIGINT      PRIMARY KEY REFERENCES chats(chat_id) ON DELETE CASCADE,
    captcha_enabled       BOOLEAN     NOT NULL DEFAULT TRUE,
    captcha_lifetime_secs INTEGER     NOT NULL DEFAULT 60    CHECK (captcha_lifetime_secs > 0),
    captcha_attempts      SMALLINT    NOT NULL DEFAULT 5     CHECK (captcha_attempts > 0),
    spam_enabled          BOOLEAN     NOT NULL DEFAULT TRUE,
    spam_threshold        REAL        NOT NULL DEFAULT 1.0   CHECK (spam_threshold >= 0),
    spam_weights          JSONB       NOT NULL DEFAULT '{}'::jsonb,
    cas_enabled           BOOLEAN     NOT NULL DEFAULT TRUE,
    clown_chance          SMALLINT    NOT NULL DEFAULT 0     CHECK (clown_chance BETWEEN 0 AND 100),
    log_allowed_messages  BOOLEAN     NOT NULL DEFAULT FALSE,
    report_hour           SMALLINT    NOT NULL DEFAULT 17    CHECK (report_hour BETWEEN 0 AND 23),
    timezone              VARCHAR(64) NOT NULL DEFAULT 'UTC',
    summary_enabled       BOOLEAN     NOT NULL DEFAULT FALSE,
    summary_token_budget  INTEGER     NOT NULL DEFAULT 50000 CHECK (summary_token_budget > 0),
    created_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE TRIGGER trg_chat_config_updated_at
    BEFORE UPDATE ON chat_config
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ── chat_moderators ──────────────────────────────────────────────────────
CREATE TABLE chat_moderators (
    chat_id    BIGINT      NOT NULL REFERENCES chats(chat_id) ON DELETE CASCADE,
    user_id    BIGINT      NOT NULL,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    granted_by BIGINT,
    PRIMARY KEY (chat_id, user_id)
);

-- ── verified_users ───────────────────────────────────────────────────────
-- Per-chat verification (one chat verified ≠ all chats verified).
CREATE TABLE verified_users (
    chat_id     BIGINT      NOT NULL REFERENCES chats(chat_id) ON DELETE CASCADE,
    user_id     BIGINT      NOT NULL,
    verified_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (chat_id, user_id)
);

-- ── captcha_challenges ───────────────────────────────────────────────────
CREATE TABLE captcha_challenges (
    id                  UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    chat_id             BIGINT      NOT NULL REFERENCES chats(chat_id) ON DELETE CASCADE,
    user_id             BIGINT      NOT NULL,
    solution            VARCHAR(8)  NOT NULL,
    attempts_left       SMALLINT    NOT NULL,
    telegram_message_id BIGINT,
    expires_at          TIMESTAMPTZ NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (chat_id, user_id)
);
CREATE INDEX idx_captcha_challenges_expires_at ON captcha_challenges (expires_at);

-- ── spam_messages ────────────────────────────────────────────────────────
-- xxh3-64 (BIGINT) of normalized body → known-spam signature.
CREATE TABLE spam_messages (
    xxh3_hash   BIGINT      PRIMARY KEY,
    sample_body TEXT        NOT NULL,
    hit_count   BIGINT      NOT NULL DEFAULT 1,
    first_seen  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_spam_messages_last_seen ON spam_messages (last_seen);

-- ── moderation_actions ───────────────────────────────────────────────────
-- Append-only audit log. UNIQUE (chat_id, target_user_id, action, message_id)
-- is the idempotency anchor: re-processing the same Telegram update never
-- double-bans / double-deletes / double-verifies.
CREATE TABLE moderation_actions (
    id             UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    chat_id        BIGINT      NOT NULL REFERENCES chats(chat_id) ON DELETE CASCADE,
    target_user_id BIGINT      NOT NULL,
    action         TEXT        NOT NULL CHECK (action IN
                       ('ban', 'unban', 'mute', 'unmute', 'delete', 'verify', 'unverify')),
    actor_kind     TEXT        NOT NULL CHECK (actor_kind IN ('bot', 'moderator')),
    actor_user_id  BIGINT,
    message_id     BIGINT,
    reason         TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (chat_id, target_user_id, action, message_id)
);
CREATE INDEX idx_moderation_actions_chat_created
    ON moderation_actions (chat_id, created_at DESC);

-- ── report_messages ──────────────────────────────────────────────────────
-- Tracks the Telegram message_id of the last posted report per chat per kind,
-- so a regenerate can delete-and-replace.
CREATE TABLE report_messages (
    chat_id             BIGINT      NOT NULL REFERENCES chats(chat_id) ON DELETE CASCADE,
    kind                TEXT        NOT NULL CHECK (kind IN ('daily')),
    telegram_message_id BIGINT      NOT NULL,
    generated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (chat_id, kind)
);

-- ── daily_stats ──────────────────────────────────────────────────────────
-- Pre-aggregated counters, written via INSERT ... ON CONFLICT DO UPDATE.
CREATE TABLE daily_stats (
    chat_id BIGINT NOT NULL REFERENCES chats(chat_id) ON DELETE CASCADE,
    date    DATE   NOT NULL,
    kind    TEXT   NOT NULL,
    value   BIGINT NOT NULL,
    PRIMARY KEY (chat_id, date, kind)
);
CREATE INDEX idx_daily_stats_chat_date ON daily_stats (chat_id, date DESC);

-- ── chat_info_cache ──────────────────────────────────────────────────────
CREATE TABLE chat_info_cache (
    chat_id       BIGINT      PRIMARY KEY REFERENCES chats(chat_id) ON DELETE CASCADE,
    title         TEXT        NOT NULL,
    type          TEXT        NOT NULL CHECK (type IN ('private', 'group', 'supergroup', 'channel')),
    description   TEXT,
    members_count INTEGER,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE TRIGGER trg_chat_info_cache_updated_at
    BEFORE UPDATE ON chat_info_cache
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ── allowed_messages ─────────────────────────────────────────────────────
-- Optional, gated by chat_config.log_allowed_messages = FALSE by default.
-- Stays empty until a chat opts in to message logging (used by AI-summary
-- pipeline + dashboard activity timeline). Has its own retention sweep.
CREATE TABLE allowed_messages (
    chat_id    BIGINT      NOT NULL REFERENCES chats(chat_id) ON DELETE CASCADE,
    message_id BIGINT      NOT NULL,
    user_id    BIGINT      NOT NULL,
    username   VARCHAR(64),
    kind       TEXT        NOT NULL,
    length     INTEGER,
    content    TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (chat_id, message_id)
);
CREATE INDEX idx_allowed_messages_chat_created ON allowed_messages (chat_id, created_at DESC);

COMMIT;
