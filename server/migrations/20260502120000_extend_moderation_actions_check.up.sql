-- Extend `moderation_actions.action` CHECK to cover M1 captcha-pipeline outcomes.
--
-- M0 shipped the audit ledger with the initial enum-like CHECK over the seven
-- baseline actions. M1's captcha pipeline introduces three new ones that the
-- expiry job and the callback handler need to record:
--
--   * `captcha_expired`  — challenge timed out, user kicked.
--   * `captcha_failed`   — final wrong attempt, user kicked.
--   * `kick`             — actual kick API call (kick = ban + unban) so the
--                          uniqueness key on (chat_id, target_user_id, action,
--                          message_id) keeps re-runs idempotent.
--
-- Postgres has no `ALTER ... ADD VALUE` for CHECK constraints, so we drop and
-- re-add. Wrapping in BEGIN/COMMIT keeps the table without a CHECK only inside
-- the transaction.

BEGIN;

ALTER TABLE moderation_actions
    DROP CONSTRAINT moderation_actions_action_check;

ALTER TABLE moderation_actions
    ADD CONSTRAINT moderation_actions_action_check
    CHECK (action IN (
        'ban', 'unban', 'mute', 'unmute', 'delete', 'verify', 'unverify',
        'captcha_expired', 'captcha_failed', 'kick'
    ));

COMMIT;
