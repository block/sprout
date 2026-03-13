ALTER TABLE reactions
    ADD COLUMN reaction_event_id VARBINARY(32) NULL AFTER emoji;

CREATE INDEX idx_reactions_source_event ON reactions (reaction_event_id);
