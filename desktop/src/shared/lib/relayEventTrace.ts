import { getThreadReference } from "@/features/messages/lib/threading";
import type { RelayEvent } from "@/shared/api/types";
import {
  KIND_DELETION,
  KIND_REACTION,
  KIND_STREAM_MESSAGE,
  KIND_STREAM_MESSAGE_DIFF,
  KIND_STREAM_MESSAGE_EDIT,
  KIND_SYSTEM_MESSAGE,
  KIND_TYPING_INDICATOR,
} from "@/shared/constants/kinds";

const TRACE_ENDPOINT =
  "http://127.0.0.1:7876/ingest/91194c04-63f1-418a-b2a7-6e113337121c";
const TRACE_SESSION_ID = "ec51df";
const TRACE_RUN_ID = "message-kind-trace";

type TraceData = Record<string, unknown>;

export function getRelayKindLabel(kind: number): string {
  switch (kind) {
    case KIND_DELETION:
      return "deletion";
    case KIND_REACTION:
      return "reaction";
    case KIND_TYPING_INDICATOR:
      return "typing_indicator";
    case KIND_STREAM_MESSAGE:
      return "stream_message";
    case 40001:
      return "legacy_stream_message";
    case KIND_STREAM_MESSAGE_EDIT:
      return "stream_message_edit";
    case KIND_STREAM_MESSAGE_DIFF:
      return "stream_message_diff";
    case KIND_SYSTEM_MESSAGE:
      return "system_message";
    default:
      return `kind_${kind}`;
  }
}

export function getThreadScopeLabel(tags: string[][]): string {
  const thread = getThreadReference(tags);
  if (!thread.rootId) {
    return "root_message";
  }
  if (!thread.parentId || thread.parentId === thread.rootId) {
    return "direct_thread_reply";
  }
  return "nested_thread_reply";
}

export function getRelayEventTraceData(event: RelayEvent): TraceData {
  const thread = getThreadReference(event.tags);
  return {
    eventId: event.id,
    kind: event.kind,
    kindLabel: getRelayKindLabel(event.kind),
    threadScope: getThreadScopeLabel(event.tags),
    rootEventId: thread.rootId,
    parentEventId: thread.parentId,
    tags: event.tags,
  };
}

export function traceRelayEvent(
  hypothesisId: string,
  location: string,
  message: string,
  data: TraceData,
) {
  console.info(`[message-kind-trace] ${message}`, data);
  try {
    fetch(TRACE_ENDPOINT, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Debug-Session-Id": TRACE_SESSION_ID,
      },
      body: JSON.stringify({
        sessionId: TRACE_SESSION_ID,
        runId: TRACE_RUN_ID,
        hypothesisId,
        location,
        message,
        data,
        timestamp: Date.now(),
      }),
    }).catch(() => {});
  } catch {}
}
