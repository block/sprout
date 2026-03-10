-- Add participant_hash column to channels for DM deduplication.
--
-- DMs are identified by the SHA-256 of their sorted participant pubkeys.
-- The unique index ensures that the same participant set maps to exactly one DM.

ALTER TABLE channels
  ADD COLUMN participant_hash VARBINARY(32) AFTER max_members;

CREATE UNIQUE INDEX idx_channels_dm_hash
  ON channels (participant_hash);
