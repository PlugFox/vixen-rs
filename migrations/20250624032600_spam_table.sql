-- Table with normalized spam messages
-- that are used to detect spam in chats.
CREATE TABLE IF NOT EXISTS spam (
    -- Length of the normalized spam message
    length INTEGER NOT NULL,

    -- Hash of the normalized spam message
    -- e.g. blake3 or xxHash3 or sha256
    hash TEXT NOT NULL,

    -- Normalized spam message
    normalized TEXT NOT NULL,

    PRIMARY KEY (length, hash)
) STRICT, WITHOUT ROWID;
