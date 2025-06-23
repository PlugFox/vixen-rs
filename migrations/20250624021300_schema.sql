-- Key-Value table
CREATE TABLE IF NOT EXISTS key_value (
    -- req Key
    k TEXT NOT NULL PRIMARY KEY,

    -- req Value
    v NOT NULL,

    -- req Created date (unixtime in seconds)
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),

    -- req Updated date (unixtime in seconds)
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')) CHECK(updated_at >= created_at)
) WITHOUT ROWID;

-- Indexes
CREATE INDEX IF NOT EXISTS kv_created_at_idx ON key_value (created_at);
CREATE INDEX IF NOT EXISTS kv_updated_at_idx ON key_value (updated_at);

-- Triggers
/* CREATE TRIGGER IF NOT EXISTS kv_meta_updated_at_trig AFTER UPDATE ON kv_tbl
    BEGIN
        UPDATE kv_tbl SET meta_updated_at = strftime('%s', 'now') WHERE k = NEW.k;
    END; */


-- Captcha buffer table
-- This table is used to store captcha data temporarily before sending it to users
CREATE TABLE captcha_buffer (
    -- Primary key with auto-increment
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,

    -- Captcha digits
    solution TEXT NOT NULL,

    -- Captcha image data
    bytes BLOB NOT NULL
) STRICT;


-- Chat information
CREATE TABLE IF NOT EXISTS chat_info (
    -- Chat ID
    chat_id INTEGER NOT NULL PRIMARY KEY,

    -- Type of the chat, can be either “private”, “group”, “supergroup” or “channel”
    chat_type TEXT NOT NULL CHECK(chat_type IN ('private', 'group', 'supergroup', 'channel')),

    -- Optional. Title, for supergroups, channels and group chats
    title TEXT,

    -- Optional. Username, for private chats, supergroups and channels if available
    username TEXT,

    -- Optional. First name of the other party in a private chat
    first_name TEXT,

    -- Optional. Last name of the other party in a private chat
    last_name TEXT,

    -- Last updated date
    updated_at INTEGER NOT NULL
) STRICT, WITHOUT ROWID;


-- Chat members
CREATE TABLE IF NOT EXISTS users (
    -- User ID
    user_id INTEGER NOT NULL PRIMARY KEY,

    -- 1 (true), if this user is a bot
    is_bot INTEGER NOT NULL DEFAULT 0 CHECK(is_bot IN (0, 1)),

    -- User's or bot's first name
    first_name TEXT NOT NULL,

    -- Optional. User's or bot's last name
    last_name TEXT,

    -- Optional. User's or bot's username
    username TEXT,

    -- Optional. IETF language tag of the user's language
    language_code TEXT
) STRICT, WITHOUT ROWID;


-- Chat members in a chat
CREATE TABLE IF NOT EXISTS chat_user (
    -- Chat ID
    chat_id INTEGER NOT NULL,

    -- User ID
    user_id INTEGER NOT NULL,

    PRIMARY KEY (chat_id, user_id)
) STRICT;

CREATE INDEX IF NOT EXISTS chat_user_user_id_idx ON chat_user (user_id);


-- Banned users
CREATE TABLE IF NOT EXISTS users_banned (
    -- User ID
    user_id INTEGER NOT NULL PRIMARY KEY,

    -- Chat ID where the user was banned
    chat_id INTEGER NOT NULL,

    -- Date of ban
    banned_at INTEGER NOT NULL,

    -- Date of unban
    expires_at INTEGER,

    -- Reason for ban
    reason TEXT
) STRICT, WITHOUT ROWID;


-- Verified users
CREATE TABLE IF NOT EXISTS users_verified (
    -- User ID
    user_id INTEGER NOT NULL PRIMARY KEY,

    -- Chat ID where the user was verified
    chat_id INTEGER NOT NULL,

    -- Date of verification
    verified_at INTEGER NOT NULL,

    -- Reason for verification
    reason TEXT
) STRICT, WITHOUT ROWID;


-- Messages sent by users
CREATE TABLE IF NOT EXISTS messages (
    -- ID of the message the user sent
    message_id INTEGER NOT NULL PRIMARY KEY,

    -- Chat the message belongs to
    chat_id INTEGER NOT NULL,

    -- Sender of the message.
    user_id INTEGER NOT NULL,

    -- Date of the message (unixtime in seconds)
    date INTEGER NOT NULL,

    -- Type of the message
    -- “text”, “photo”, “video”, “audio”, “document”, “sticker”, “location”, “contact” or “other”
    message_type TEXT NOT NULL,

    -- Optional. For replies in the same chat and message thread, the original message.
    reply_to INTEGER,

    -- Length of the message/content
    length INTEGER NOT NULL DEFAULT 0 CHECK(length >= 0),

    content TEXT NOT NULL
) STRICT, WITHOUT ROWID;

-- Indexes for messages
CREATE INDEX IF NOT EXISTS messages_user_id_idx ON messages (user_id);
CREATE INDEX IF NOT EXISTS messages_date_idx ON messages (date);


-- Deleted messages
CREATE TABLE IF NOT EXISTS messages_deleted (
    -- ID of the message the user sent
    message_id INTEGER NOT NULL PRIMARY KEY,

    -- Deleted date (unixtime in seconds)
    deleted_at INTEGER NOT NULL,

    -- Reason for deletion
    reason TEXT
) STRICT, WITHOUT ROWID;


-- Captcha messages sent to users
CREATE TABLE messages_captcha (
    -- Primary key with auto-increment
    message_id INTEGER NOT NULL PRIMARY KEY,

     -- Whether the message is deleted or not (0 = false, 1 = true)
    deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0, 1)),

    -- Identifier for the user
    user_id INTEGER NOT NULL,

    -- Identifier for the chat
    chat_id INTEGER NOT NULL,

    -- Text of the message
    caption TEXT NOT NULL,

    -- Captcha digits
    solution TEXT NOT NULL,

    -- User input for the captcha
    input TEXT NOT NULL,

    -- Timestamp when the captcha expires
    expires_at INTEGER NOT NULL,

    -- Creation timestamp
    created_at INTEGER NOT NULL,

    -- Last update timestamp
    updated_at INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS messages_captcha_user_id_idx ON messages_captcha (user_id);
CREATE INDEX IF NOT EXISTS messages_captcha_expires_at_idx ON messages_captcha (expires_at);