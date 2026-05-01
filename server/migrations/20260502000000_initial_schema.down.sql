-- Reverse the initial schema. Drops tables in reverse-dependency order so the
-- foreign keys to chats(chat_id) come down first. The `uuid-ossp` extension is
-- left in place — it may be used by other schemas in the same database, and
-- creating it is idempotent in the up migration.

BEGIN;

DROP TABLE IF EXISTS allowed_messages;
DROP TABLE IF EXISTS chat_info_cache;
DROP TABLE IF EXISTS daily_stats;
DROP TABLE IF EXISTS report_messages;
DROP TABLE IF EXISTS moderation_actions;
DROP TABLE IF EXISTS spam_messages;
DROP TABLE IF EXISTS captcha_challenges;
DROP TABLE IF EXISTS verified_users;
DROP TABLE IF EXISTS chat_moderators;
DROP TABLE IF EXISTS chat_config;
DROP TABLE IF EXISTS chats;

DROP FUNCTION IF EXISTS update_updated_at();

COMMIT;
