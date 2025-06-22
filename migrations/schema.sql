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
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT, -- Primary key with auto-increment
    solution TEXT NOT NULL, -- Captcha digits
    bytes BLOB NOT NULL -- Captcha image data
) STRICT;


-- Captcha messages sent to users
CREATE TABLE captcha_message (
    message_id INTEGER NOT NULL PRIMARY KEY, -- Primary key with auto-increment
    deleted INTEGER NOT NULL DEFAULT 0, -- Whether the message is deleted or not (0 = false, 1 = true)
    user_id INTEGER NOT NULL, -- Identifier for the user
    chat_id INTEGER NOT NULL, -- Identifier for the chat
    caption TEXT NOT NULL, -- Text of the message
    solution TEXT NOT NULL, -- Captcha digits
    input TEXT NOT NULL, -- User input for the captcha
    expires_at INTEGER NOT NULL, -- Timestamp when the captcha expires
    created_at INTEGER NOT NULL, -- Creation timestamp
    updated_at INTEGER NOT NULL -- Last update timestamp
) STRICT, WITHOUT ROWID;;

CREATE INDEX IF NOT EXISTS captcha_message_chat_id_idx ON captcha_message (chat_id);
CREATE INDEX IF NOT EXISTS captcha_message_user_id_idx ON captcha_message (user_id);


-- Chat information
CREATE TABLE IF NOT EXISTS chat_info (
    -- Chat ID
    chat_id INTEGER NOT NULL PRIMARY KEY,

    -- Type of the chat, can be either “private”, “group”, “supergroup” or “channel”
    chat_type TEXT NOT NULL,

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


