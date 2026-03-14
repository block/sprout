-- Migrate legacy kind:40001 (pre-NIP-29 stream messages) to kind:9 (NIP-29 group chat).
-- This is a one-time migration. The application code already uses kind:9 for new messages.
-- The desktop client includes 40001 in CHANNEL_EVENT_KINDS for backward compat during
-- the transition window; this migration makes that unnecessary once complete.
UPDATE events SET kind = 9 WHERE kind = 40001;
