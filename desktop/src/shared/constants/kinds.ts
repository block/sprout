export const KIND_DELETION = 5;
export const KIND_REACTION = 7;
export const KIND_STREAM_MESSAGE = 9;
export const KIND_STREAM_MESSAGE_V2 = 40002;
export const KIND_STREAM_MESSAGE_EDIT = 40003;
export const KIND_STREAM_MESSAGE_DIFF = 40008;
export const KIND_SYSTEM_MESSAGE = 40099;

export const CHANNEL_EVENT_KINDS = [
  KIND_DELETION, // 5 — NIP-09 event deletions
  KIND_REACTION, // 7 — NIP-25 reactions
  KIND_STREAM_MESSAGE, // 9 — NIP-29 group chat messages
  40001, // legacy: pre-migration stream messages
  KIND_STREAM_MESSAGE_DIFF, // 40008 — message diffs
  KIND_SYSTEM_MESSAGE, // 40099 — system messages (join, leave, etc.)
] as const;
