CREATE TABLE IF NOT EXISTS thread_metadata (
    event_created_at        DATETIME(6)   NOT NULL,
    event_id                VARBINARY(32) NOT NULL,
    channel_id              BINARY(16)    NOT NULL,
    parent_event_id         VARBINARY(32),
    parent_event_created_at DATETIME(6),
    root_event_id           VARBINARY(32),
    root_event_created_at   DATETIME(6),
    depth                   INT           NOT NULL DEFAULT 0,
    reply_count             INT           NOT NULL DEFAULT 0,
    descendant_count        INT           NOT NULL DEFAULT 0,
    last_reply_at           DATETIME(6),
    broadcast               TINYINT(1)    NOT NULL DEFAULT 0,
    PRIMARY KEY (event_created_at, event_id),
    CONSTRAINT fk_thread_channel FOREIGN KEY (channel_id) REFERENCES channels(id)
);

CREATE INDEX idx_thread_parent        ON thread_metadata (parent_event_id);
CREATE INDEX idx_thread_root          ON thread_metadata (root_event_id);
CREATE INDEX idx_thread_channel_depth ON thread_metadata (channel_id, depth, event_created_at);
CREATE INDEX idx_thread_event_id      ON thread_metadata (event_id);
