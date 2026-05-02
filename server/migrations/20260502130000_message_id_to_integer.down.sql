-- Widen `telegram_message_id` columns back to BIGINT.

BEGIN;

ALTER TABLE captcha_challenges
    ALTER COLUMN telegram_message_id TYPE BIGINT USING telegram_message_id::BIGINT;

ALTER TABLE moderation_actions
    ALTER COLUMN message_id TYPE BIGINT USING message_id::BIGINT;

COMMIT;
