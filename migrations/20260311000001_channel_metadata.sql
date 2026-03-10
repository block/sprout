-- Add topic and purpose fields to channels table.
--
-- topic:       Current channel topic (short, visible in header).
-- topic_set_by: Pubkey of the user who last set the topic.
-- topic_set_at: When the topic was last set.
-- purpose:     Channel purpose / description of intent.
-- purpose_set_by: Pubkey of the user who last set the purpose.
-- purpose_set_at: When the purpose was last set.

ALTER TABLE channels
  ADD COLUMN topic          TEXT          AFTER description,
  ADD COLUMN topic_set_by   VARBINARY(32) AFTER topic,
  ADD COLUMN topic_set_at   DATETIME(6)   AFTER topic_set_by,
  ADD COLUMN purpose        TEXT          AFTER topic_set_at,
  ADD COLUMN purpose_set_by VARBINARY(32) AFTER purpose,
  ADD COLUMN purpose_set_at DATETIME(6)   AFTER purpose_set_by;
