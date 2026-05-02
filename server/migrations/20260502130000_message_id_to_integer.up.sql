-- Narrow `telegram_message_id` columns from BIGINT to INTEGER.
--
-- Telegram `MessageId` is `i32` in teloxide, so the previous BIGINT columns
-- forced a lossy `mid as i32` cast at every read site. Schema-level INTEGER
-- removes the cast and makes the type contract honest.

BEGIN;

ALTER TABLE captcha_challenges
    ALTER COLUMN telegram_message_id TYPE INTEGER USING telegram_message_id::INTEGER;

ALTER TABLE moderation_actions
    ALTER COLUMN message_id TYPE INTEGER USING message_id::INTEGER;

COMMIT;
