-- Sprout initial schema — MySQL 8.0
-- Monthly range partitioning on events.created_at and delivery_log.delivered_at
-- Run via: sqlx migrate run --database-url $DATABASE_URL

-- ─── Channels ────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS channels (
    id              BINARY(16)      NOT NULL,
    name            TEXT            NOT NULL,
    channel_type    ENUM('stream','forum','dm','workflow') NOT NULL DEFAULT 'stream',
    visibility      ENUM('open','private') NOT NULL DEFAULT 'private',
    description     TEXT,
    canvas          TEXT,
    created_by      VARBINARY(32)   NOT NULL,
    created_at      DATETIME(6)     NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at      DATETIME(6)     NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    archived_at     DATETIME(6),
    deleted_at      DATETIME(6),
    nip29_group_id  VARCHAR(255)    UNIQUE,
    topic_required  TINYINT(1)      NOT NULL DEFAULT 1,
    max_members     INT,
    PRIMARY KEY (id),
    CONSTRAINT name_not_empty CHECK (LENGTH(name) > 0)
);

CREATE INDEX idx_channels_type       ON channels (channel_type);
CREATE INDEX idx_channels_visibility ON channels (visibility);
CREATE INDEX idx_channels_created_by ON channels (created_by);
-- Note: GIN/full-text index on name+description omitted; use MySQL FULLTEXT or app-layer search

-- ─── Channel Members ─────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS channel_members (
    channel_id  BINARY(16)      NOT NULL,
    pubkey      VARBINARY(32)   NOT NULL,
    role        ENUM('owner','admin','member','guest','bot') NOT NULL DEFAULT 'member',
    joined_at   DATETIME(6)     NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    invited_by  VARBINARY(32),
    removed_at  DATETIME(6),
    removed_by  VARBINARY(32),
    PRIMARY KEY (channel_id, pubkey),
    CONSTRAINT fk_channel_members_channel
        FOREIGN KEY (channel_id) REFERENCES channels(id) ON DELETE CASCADE
);

-- Note: Partial indexes (WHERE removed_at IS NULL) not supported in MySQL.
-- Using regular indexes instead.
CREATE INDEX idx_channel_members_pubkey  ON channel_members (pubkey);
CREATE INDEX idx_channel_members_channel ON channel_members (channel_id);

-- ─── Users ───────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS users (
    pubkey              VARBINARY(32)   NOT NULL,
    nip05_handle        VARCHAR(255)    UNIQUE,
    display_name        TEXT,
    avatar_url          TEXT,
    agent_type          VARCHAR(255),
    capabilities        JSON,
    okta_user_id        VARCHAR(255)    UNIQUE,
    created_at          DATETIME(6)     NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at          DATETIME(6)     NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    deactivated_at      DATETIME(6),
    metadata_event_id   VARBINARY(32),
    -- NOTE: No FK to events — events is partitioned; MySQL does not support FK
    -- from/to partitioned tables. Integrity enforced at application layer.
    PRIMARY KEY (pubkey),
    CONSTRAINT pubkey_length CHECK (LENGTH(pubkey) = 32)
);

-- Note: Partial indexes not supported in MySQL. Using regular indexes.
CREATE INDEX idx_users_nip05      ON users (nip05_handle);
CREATE INDEX idx_users_agent_type ON users (agent_type);
CREATE INDEX idx_users_okta       ON users (okta_user_id);

-- ─── Events (Partitioned by Month) ───────────────────────────────────────────

-- Partitioned by RANGE on TO_DAYS(created_at).
-- ⚠️  MySQL requires the partition key to be part of every unique index / PK.
--     PK is (created_at, id) to satisfy this requirement.
-- ⚠️  MySQL does not support FK constraints on partitioned tables.
--     channel_id → channels(id) is enforced at the application layer.
-- ⚠️  Deduplication by id alone is not enforceable via unique index across
--     partitions. SHA-256 collision resistance + app-layer INSERT IGNORE used.

CREATE TABLE IF NOT EXISTS events (
    id          VARBINARY(32)   NOT NULL,
    pubkey      VARBINARY(32)   NOT NULL,
    created_at  DATETIME(6)     NOT NULL,
    kind        INT             NOT NULL,
    tags        JSON            NOT NULL,
    content     TEXT            NOT NULL,
    sig         VARBINARY(64)   NOT NULL,
    received_at DATETIME(6)     NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    channel_id  BINARY(16),
    -- No FK: channel_id REFERENCES channels(id) — not supported on partitioned tables
    PRIMARY KEY (created_at, id)
)
PARTITION BY RANGE (TO_DAYS(created_at)) (
    PARTITION p2026_01 VALUES LESS THAN (TO_DAYS('2026-02-01')),
    PARTITION p2026_02 VALUES LESS THAN (TO_DAYS('2026-03-01')),
    PARTITION p2026_03 VALUES LESS THAN (TO_DAYS('2026-04-01')),
    PARTITION p2026_04 VALUES LESS THAN (TO_DAYS('2026-05-01')),
    PARTITION p2026_05 VALUES LESS THAN (TO_DAYS('2026-06-01')),
    PARTITION p2026_06 VALUES LESS THAN (TO_DAYS('2026-07-01'))
);

-- Composite index: pubkey + kind + created_at (NIP-01 author+kind queries)
-- created_at must be leftmost or included for partition pruning
CREATE INDEX idx_events_pubkey_kind_created ON events (pubkey, kind, created_at);

-- Composite index: channel + created_at (channel message pagination)
-- Note: Partial index (WHERE channel_id IS NOT NULL) not supported; regular index used
CREATE INDEX idx_events_channel_created ON events (channel_id, created_at);

-- Composite index: kind + created_at (kind-only queries)
CREATE INDEX idx_events_kind_created ON events (kind, created_at);

-- Note: GIN index on tags JSON omitted — no equivalent in MySQL.
-- For tag filtering, add generated columns + regular indexes as needed at app layer.

-- ─── Persistent Subscriptions ────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS subscriptions (
    id                      VARCHAR(255)    NOT NULL,
    name                    TEXT            NOT NULL,
    owner_pubkey            VARBINARY(32)   NOT NULL,
    filter_channel_ids      JSON,
    filter_topics           JSON,
    filter_authors          JSON,
    filter_kinds            JSON,
    filter_content_regex    TEXT,
    filter_tags             JSON,
    delivery_method         ENUM('websocket','webhook','email_digest','push') NOT NULL,
    delivery_url            TEXT,
    delivery_secret         TEXT,
    delivery_retry_max      INT             NOT NULL DEFAULT 3,
    delivery_email_frequency TEXT,
    delivery_email_send_at  TIME,
    delivery_email_timezone TEXT,
    status                  ENUM('active','paused','deleted') NOT NULL DEFAULT 'active',
    pause_reason            ENUM('manual','circuit_breaker','rate_limit','admin'),
    paused_at               DATETIME(6),
    visibility              VARCHAR(50)     NOT NULL DEFAULT 'private',
    total_matched           BIGINT          NOT NULL DEFAULT 0,
    total_delivered         BIGINT          NOT NULL DEFAULT 0,
    last_matched_at         DATETIME(6),
    created_at              DATETIME(6)     NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at              DATETIME(6)     NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    CONSTRAINT fk_subscriptions_owner
        FOREIGN KEY (owner_pubkey) REFERENCES users(pubkey)
);

-- Note: Partial indexes not supported. Regular indexes used.
-- Note: GIN indexes on JSON arrays (filter_kinds, filter_channel_ids) omitted.
--       Use JSON_CONTAINS() at query time or add generated columns for hot paths.
CREATE INDEX idx_subscriptions_owner  ON subscriptions (owner_pubkey);
CREATE INDEX idx_subscriptions_status ON subscriptions (status);

-- ─── Delivery Log (Partitioned) ───────────────────────────────────────────────

-- AUTO_INCREMENT replaces CREATE SEQUENCE + nextval().
-- ⚠️  PK must include partition key (delivered_at) — using (delivered_at, id).
-- ⚠️  MySQL does not support FK constraints on partitioned tables.
--     subscription_id → subscriptions(id) enforced at application layer.

CREATE TABLE IF NOT EXISTS delivery_log (
    id              BIGINT          NOT NULL AUTO_INCREMENT,
    subscription_id VARCHAR(255)    NOT NULL,
    event_id        VARBINARY(32)   NOT NULL,
    delivered_at    DATETIME(6)     NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    method          ENUM('websocket','webhook','email_digest','push') NOT NULL,
    success         TINYINT(1)      NOT NULL,
    http_status     INT,
    error_message   TEXT,
    attempt_number  SMALLINT        NOT NULL DEFAULT 1,
    -- No FK: subscription_id REFERENCES subscriptions(id) — not supported on partitioned tables
    PRIMARY KEY (delivered_at, id),
    KEY (id)  -- MySQL requires AUTO_INCREMENT column to be a key
)
PARTITION BY RANGE (TO_DAYS(delivered_at)) (
    PARTITION p2026_03 VALUES LESS THAN (TO_DAYS('2026-04-01')),
    PARTITION p2026_04 VALUES LESS THAN (TO_DAYS('2026-05-01')),
    PARTITION p2026_05 VALUES LESS THAN (TO_DAYS('2026-06-01')),
    PARTITION p2026_06 VALUES LESS THAN (TO_DAYS('2026-07-01'))
);

CREATE INDEX idx_delivery_log_sub_delivered ON delivery_log (subscription_id, delivered_at);
-- Note: Partial index (WHERE success = FALSE) not supported; regular index used
CREATE INDEX idx_delivery_log_failures      ON delivery_log (subscription_id, delivered_at, success);

-- ─── Workflows ────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS workflows (
    id                  BINARY(16)      NOT NULL,
    name                TEXT            NOT NULL,
    owner_pubkey        VARBINARY(32)   NOT NULL,
    channel_id          BINARY(16),
    definition          JSON            NOT NULL,
    definition_hash     VARBINARY(32)   NOT NULL,
    status              ENUM('pending','running','waiting_approval','completed','failed','cancelled') NOT NULL DEFAULT 'pending',
    trigger_event_id    VARBINARY(32),
    current_step        INT             NOT NULL DEFAULT 0,
    execution_trace     JSON            NOT NULL,
    started_at          DATETIME(6),
    completed_at        DATETIME(6),
    failed_at           DATETIME(6),
    error_message       TEXT,
    created_at          DATETIME(6)     NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at          DATETIME(6)     NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    CONSTRAINT fk_workflows_owner
        FOREIGN KEY (owner_pubkey) REFERENCES users(pubkey),
    CONSTRAINT fk_workflows_channel
        FOREIGN KEY (channel_id) REFERENCES channels(id)
);

CREATE INDEX idx_workflows_owner   ON workflows (owner_pubkey);
-- Note: Partial index (WHERE status IN (...)) not supported; regular index used
CREATE INDEX idx_workflows_status  ON workflows (status);
-- Note: Partial index (WHERE channel_id IS NOT NULL) not supported; regular index used
CREATE INDEX idx_workflows_channel ON workflows (channel_id);

-- ─── API Tokens ───────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS api_tokens (
    id              BINARY(16)      NOT NULL,
    token_hash      VARBINARY(32)   NOT NULL UNIQUE,
    owner_pubkey    VARBINARY(32)   NOT NULL,
    name            TEXT            NOT NULL,
    scopes          JSON            NOT NULL,
    channel_ids     JSON,
    created_at      DATETIME(6)     NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    expires_at      DATETIME(6),
    last_used_at    DATETIME(6),
    revoked_at      DATETIME(6),
    revoked_by      VARBINARY(32),
    PRIMARY KEY (id),
    CONSTRAINT token_hash_length CHECK (LENGTH(token_hash) = 32),
    CONSTRAINT fk_api_tokens_owner
        FOREIGN KEY (owner_pubkey) REFERENCES users(pubkey)
);

-- Note: Partial indexes (WHERE revoked_at IS NULL) not supported; regular indexes used
CREATE INDEX idx_api_tokens_owner ON api_tokens (owner_pubkey);
CREATE INDEX idx_api_tokens_hash  ON api_tokens (token_hash);

-- ─── Rate Limit Violations ────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS rate_limit_violations (
    id              BIGINT          NOT NULL AUTO_INCREMENT,
    pubkey          VARBINARY(32)   NOT NULL,
    violation_at    DATETIME(6)     NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    limit_type      TEXT            NOT NULL,
    limit_value     INT             NOT NULL,
    actual_value    INT             NOT NULL,
    action_taken    TEXT            NOT NULL,
    PRIMARY KEY (id)
);

CREATE INDEX idx_rate_violations_pubkey_time ON rate_limit_violations (pubkey, violation_at);
