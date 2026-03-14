-- Pubkey allowlist for NIP-42 authentication.
-- When SPROUT_PUBKEY_ALLOWLIST=true, pubkeys in this table may authenticate
-- via NIP-42 keypair-only (no JWT or API token required).
CREATE TABLE IF NOT EXISTS pubkey_allowlist (
    pubkey      VARBINARY(32)   NOT NULL,
    added_by    VARBINARY(32),
    added_at    DATETIME(6)     NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    note        TEXT,
    PRIMARY KEY (pubkey)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
