-- Denormalized mentions table for indexed p-tag lookups.
-- Replaces JSON_CONTAINS full-table scans in feed queries.
CREATE TABLE IF NOT EXISTS event_mentions (
    pubkey_hex VARCHAR(64) NOT NULL,
    event_id VARBINARY(32) NOT NULL,
    event_created_at DATETIME(6) NOT NULL,
    channel_id BINARY(16) NULL,
    event_kind INT UNSIGNED NOT NULL,
    PRIMARY KEY (pubkey_hex, event_id),
    INDEX idx_mentions_pubkey_time (pubkey_hex, event_created_at DESC),
    INDEX idx_mentions_pubkey_kind_time (pubkey_hex, event_kind, event_created_at DESC)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- Backfill existing events that have p-tags.
-- JSON_TABLE extracts each tag sub-array; we filter for p-tags.
INSERT IGNORE INTO event_mentions (pubkey_hex, event_id, event_created_at, channel_id, event_kind)
SELECT
    LOWER(jt.pubkey_hex),
    e.id,
    e.created_at,
    e.channel_id,
    e.kind
FROM events e,
JSON_TABLE(
    e.tags,
    '$[*]' COLUMNS (
        tag_name VARCHAR(10) PATH '$[0]',
        pubkey_hex VARCHAR(64) PATH '$[1]'
    )
) AS jt
WHERE jt.tag_name = 'p'
  AND jt.pubkey_hex IS NOT NULL
  AND LENGTH(jt.pubkey_hex) = 64
  AND jt.pubkey_hex REGEXP '^[0-9a-fA-F]{64}$'
  AND e.deleted_at IS NULL;
