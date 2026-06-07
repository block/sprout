-- Migration 0002: relay viewer channel allowlist
--
-- Adds a read-only relay role (`viewer`) and a per-viewer explicit channel
-- allowlist. Viewer enforcement is done by constructing a restricted auth
-- context from this table after relay membership admission.

-- ── 1. Allow relay_members.role = 'viewer' ──────────────────────────────────

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'relay_members_role_check'
    ) THEN
        ALTER TABLE relay_members DROP CONSTRAINT relay_members_role_check;
    END IF;

    ALTER TABLE relay_members
        ADD CONSTRAINT relay_members_role_check
        CHECK (role IN ('owner', 'admin', 'member', 'viewer'));
END $$;

-- ── 2. Per-viewer channel allowlist ─────────────────────────────────────────

CREATE TABLE IF NOT EXISTS relay_member_channel_allowlist (
    pubkey      TEXT NOT NULL REFERENCES relay_members(pubkey) ON DELETE CASCADE,
    channel_id  UUID NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    added_by    TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (pubkey, channel_id)
);

CREATE INDEX IF NOT EXISTS idx_relay_member_channel_allowlist_channel
    ON relay_member_channel_allowlist(channel_id);
