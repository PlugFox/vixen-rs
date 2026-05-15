-- M5: indexes for moderator-dashboard keyset pagination.
--
-- `/api/v1/chats/{id}/moderation/verified` orders by `verified_at DESC` with
-- `user_id DESC` as the tie-breaker. The base table has only `PRIMARY KEY
-- (chat_id, user_id)` — without this composite index the dashboard would
-- full-scan the partition on every page.
--
-- `idx_moderation_actions_chat_created` from the initial schema already
-- covers the audit-log pagination; we add `id` as a tie-breaker in the
-- ORDER BY at the query level, which the planner resolves with a cheap
-- in-memory sort over the index range.

BEGIN;

CREATE INDEX idx_verified_users_chat_verified_at
    ON verified_users (chat_id, verified_at DESC, user_id DESC);

COMMIT;
