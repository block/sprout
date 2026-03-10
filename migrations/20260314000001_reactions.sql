CREATE TABLE IF NOT EXISTS reactions (
    event_created_at DATETIME(6)  NOT NULL,
    event_id         VARBINARY(32) NOT NULL,
    pubkey           VARBINARY(32) NOT NULL,
    emoji            VARCHAR(64)   NOT NULL,
    created_at       DATETIME(6)   NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    removed_at       DATETIME(6),
    PRIMARY KEY (event_created_at, event_id, pubkey, emoji)
);

CREATE INDEX idx_reactions_event  ON reactions (event_id, event_created_at);
CREATE INDEX idx_reactions_pubkey ON reactions (pubkey);
