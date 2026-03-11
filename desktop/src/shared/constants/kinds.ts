export const KIND_STREAM_MESSAGE = 40001;
export const KIND_STREAM_MESSAGE_V2 = 40002;
export const KIND_STREAM_MESSAGE_EDIT = 40003;
export const KIND_STREAM_MESSAGE_DIFF = 40008;

export const STREAM_MESSAGE_KINDS = [
  KIND_STREAM_MESSAGE,      // 40001 — was in original filter
  KIND_STREAM_MESSAGE_DIFF, // 40008 — new in this PR
] as const;
