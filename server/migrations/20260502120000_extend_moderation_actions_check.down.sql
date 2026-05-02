-- Reverse: restore the original M0 CHECK list.
--
-- Reverting requires the table to contain only baseline actions; if any rows
-- carry one of the new values the ADD CONSTRAINT will fail (which is the right
-- behaviour — silently dropping rows would lose audit data).

BEGIN;

ALTER TABLE moderation_actions
    DROP CONSTRAINT moderation_actions_action_check;

ALTER TABLE moderation_actions
    ADD CONSTRAINT moderation_actions_action_check
    CHECK (action IN (
        'ban', 'unban', 'mute', 'unmute', 'delete', 'verify', 'unverify'
    ));

COMMIT;
