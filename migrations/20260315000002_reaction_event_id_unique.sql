-- Upgrade the reaction_event_id index from non-unique to unique.
-- Prevents duplicate kind:7 events from creating orphaned reaction rows.
DROP INDEX idx_reactions_source_event ON reactions;
CREATE UNIQUE INDEX idx_reactions_source_event ON reactions (reaction_event_id);
