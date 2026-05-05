-- Migration 0001: relay_members
--
-- Introduces the relay-level membership table (NIP-43).
-- Replaces the old pubkey_allowlist with a richer model that tracks role
-- (owner / admin / member), who added the entry, and timestamps.
--
-- Idempotent: safe to run more than once.

-- ── 1. Create relay_members ───────────────────────────────────────────────────

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public'
          AND table_name   = 'relay_members'
    ) THEN
        CREATE TABLE relay_members (
            pubkey      TEXT PRIMARY KEY,
            role        TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member')),
            added_by    TEXT,
            created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE INDEX idx_relay_members_role ON relay_members(role);
    END IF;
END $$;

-- ── 2. Migrate existing allowlist rows ────────────────────────────────────────
--
-- pubkey_allowlist stores pubkeys as BYTEA and timestamps as added_at.
-- Convert BYTEA to lowercase hex text via encode(..., 'hex').

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public'
          AND table_name   = 'pubkey_allowlist'
    ) THEN
        INSERT INTO relay_members (pubkey, role, added_by, created_at)
        SELECT encode(pubkey, 'hex'), 'member', NULL, added_at
        FROM pubkey_allowlist
        ON CONFLICT (pubkey) DO NOTHING;
    END IF;
END $$;

-- NOTE: pubkey_allowlist is intentionally NOT dropped here.
-- The old allowlist code still references it. Drop it in a future migration
-- once all references have been removed.
