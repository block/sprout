-- Composite index for addressable event replacement locking.
-- replace_addressable_event() uses SELECT ... FOR UPDATE on
-- (kind, pubkey, channel_id, deleted_at IS NULL) to serialize concurrent
-- writers for the same logical address. Without this index, InnoDB cannot
-- take a gap lock on a cold address (no existing rows), allowing two
-- concurrent first-time emissions to both insert.
--
-- The index covers the exact WHERE clause and enables next-key locking
-- on the (kind, pubkey, channel_id) range even when no active row exists.
CREATE INDEX idx_events_addressable ON events (kind, pubkey, channel_id, deleted_at);
