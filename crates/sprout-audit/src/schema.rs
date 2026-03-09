/// DDL for the `audit_log` table. Passed to [`sqlx::raw_sql`] on startup.
///
/// Note: `CREATE TABLE IF NOT EXISTS` does not alter existing tables. If the
/// live database has `event_kind SMALLINT` from an earlier schema, run
/// [`AUDIT_MIGRATE_SQL`] once to widen the column to `INT`.
pub const AUDIT_SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS audit_log (
    seq          BIGINT       NOT NULL PRIMARY KEY,
    timestamp    DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    event_id     VARCHAR(255) NOT NULL,
    event_kind   INT          NOT NULL,
    actor_pubkey VARCHAR(255) NOT NULL,
    action       VARCHAR(64)  NOT NULL,
    channel_id   BINARY(16),
    metadata     JSON         NOT NULL,
    prev_hash    VARCHAR(64)  NOT NULL,
    hash         VARCHAR(64)  NOT NULL,
    INDEX idx_audit_log_timestamp (timestamp),
    INDEX idx_audit_log_actor (actor_pubkey),
    INDEX idx_audit_log_channel (channel_id)
);
"#;

/// One-time migration: widens `event_kind` from `SMALLINT` to `INT` on databases
/// created before the column type was corrected. Safe to run on an already-`INT`
/// column — MySQL is a no-op when the type matches.
///
/// Run this manually:
/// ```sql
/// ALTER TABLE audit_log MODIFY COLUMN event_kind INT NOT NULL;
/// ```
pub const AUDIT_MIGRATE_SQL: &str = r#"
ALTER TABLE audit_log MODIFY COLUMN event_kind INT NOT NULL;
"#;
