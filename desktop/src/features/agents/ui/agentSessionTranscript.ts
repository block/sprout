import type {
  ObserverEvent,
  PromptSection,
  ToolStatus,
  TranscriptItem,
} from "./agentSessionTypes";
import {
  findSproutToolName,
  isGenericToolTitle,
  normalizeToolStatus,
} from "./agentSessionToolCatalog";
import { asRecord, asString } from "./agentSessionUtils";
import {
  describeTurnStarted,
  describeSessionResolved,
  extractBlockText,
  extractContentText,
  extractPromptText,
  extractToolArgs,
  extractToolIdentity,
  extractToolResult,
  parsePromptText,
} from "./agentSessionTranscriptHelpers";

export { describeRawEvent } from "./agentSessionTranscriptHelpers";

export function buildTranscript(events: ObserverEvent[]): TranscriptItem[] {
  const items: TranscriptItem[] = [];
  const itemsById = new Map<string, TranscriptItem>();

  // Maps a logical message ID (e.g. `assistant:msg1`) to the *actual* key
  // currently being appended to.  When a non-message item interleaves, we
  // seal the current key so subsequent chunks create a fresh entry.
  const activeMessageKey = new Map<string, string>();
  const sealedKeys = new Set<string>();
  let continuationSeq = 0;

  /** Seal every currently-open message so the next chunk starts a new entry. */
  function sealOpenMessages() {
    for (const [, currentKey] of activeMessageKey) {
      if (!sealedKeys.has(currentKey)) {
        sealedKeys.add(currentKey);
      }
    }
  }

  function upsertMessage(
    id: string,
    role: "assistant" | "user",
    title: string,
    text: string,
    timestamp: string,
  ) {
    const currentKey = activeMessageKey.get(id);

    // If there is an active (non-sealed) key, append to it.
    if (currentKey && !sealedKeys.has(currentKey)) {
      const existing = itemsById.get(currentKey);
      if (existing?.type === "message") {
        existing.text += text;
        return;
      }
    }

    // Otherwise create a new entry (either first time, or continuation).
    continuationSeq += 1;
    const newKey = currentKey ? `${id}:c${continuationSeq}` : id;
    const item: TranscriptItem = {
      id: newKey,
      type: "message",
      role,
      title,
      text,
      timestamp,
    };
    items.push(item);
    itemsById.set(newKey, item);
    activeMessageKey.set(id, newKey);
  }

  function upsertTextItem(
    id: string,
    type: "thought" | "lifecycle",
    title: string,
    text: string,
    timestamp: string,
  ) {
    const existing = itemsById.get(id);
    if (existing && existing.type === type) {
      existing.text += text;
      return;
    }
    sealOpenMessages();
    const item: TranscriptItem = { id, type, title, text, timestamp };
    items.push(item);
    itemsById.set(id, item);
  }

  function upsertMetadata(
    id: string,
    title: string,
    sections: PromptSection[],
    timestamp: string,
  ) {
    const existing = itemsById.get(id);
    if (existing?.type === "metadata") {
      existing.sections = sections;
      return;
    }
    sealOpenMessages();
    const item: TranscriptItem = {
      id,
      type: "metadata",
      title,
      sections,
      timestamp,
    };
    items.push(item);
    itemsById.set(id, item);
  }

  function upsertTool(
    id: string,
    title: string,
    toolName: string,
    sproutToolName: string | null,
    status: ToolStatus,
    args: Record<string, unknown>,
    result: string,
    isError: boolean,
    timestamp: string,
  ) {
    const existing = itemsById.get(id);
    const canonicalSproutToolName =
      sproutToolName ?? findSproutToolName(toolName, true);
    if (existing?.type === "tool") {
      if (!isGenericToolTitle(title)) {
        existing.title = title;
      }
      if (canonicalSproutToolName) {
        existing.sproutToolName = canonicalSproutToolName;
        existing.toolName = canonicalSproutToolName;
      } else if (!existing.sproutToolName && !isGenericToolTitle(toolName)) {
        existing.toolName = toolName;
      }
      existing.status = status;
      existing.args = Object.keys(args).length > 0 ? args : existing.args;
      if (result) existing.result = result;
      existing.isError = isError || existing.isError;
      if (
        (status === "completed" || status === "failed") &&
        existing.completedAt == null
      ) {
        existing.completedAt = timestamp;
      }
      return;
    }
    sealOpenMessages();
    const item: TranscriptItem = {
      id,
      type: "tool",
      title,
      toolName: canonicalSproutToolName ?? toolName,
      sproutToolName: canonicalSproutToolName,
      status,
      args,
      result,
      isError,
      timestamp,
      startedAt: timestamp,
      completedAt: null,
    };
    items.push(item);
    itemsById.set(id, item);
  }

  for (const event of events) {
    if (event.kind === "turn_started") {
      upsertTextItem(
        `turn:${event.turnId ?? event.seq}`,
        "lifecycle",
        "Turn started",
        describeTurnStarted(event.payload),
        event.timestamp,
      );
      continue;
    }

    if (event.kind === "session_resolved") {
      upsertTextItem(
        `session:${event.turnId ?? event.seq}`,
        "lifecycle",
        "Session ready",
        describeSessionResolved(event.payload),
        event.timestamp,
      );
      continue;
    }

    if (event.kind === "acp_parse_error") {
      upsertTextItem(
        `parse-error:${event.seq}`,
        "lifecycle",
        "Wire parse error",
        extractBlockText(event.payload),
        event.timestamp,
      );
      continue;
    }

    if (event.kind !== "acp_read" && event.kind !== "acp_write") {
      continue;
    }

    const payload = asRecord(event.payload);
    const method = asString(payload.method);

    if (event.kind === "acp_write" && method === "session/prompt") {
      const promptText = extractPromptText(payload);
      if (promptText) {
        const parsedPrompt = parsePromptText(promptText);
        if (parsedPrompt.userText) {
          upsertMessage(
            `prompt:${event.turnId ?? event.seq}`,
            "user",
            parsedPrompt.userTitle,
            parsedPrompt.userText,
            event.timestamp,
          );
        }
        if (parsedPrompt.sections.length > 0) {
          upsertMetadata(
            `prompt-context:${event.turnId ?? event.seq}`,
            "Prompt context",
            parsedPrompt.sections,
            event.timestamp,
          );
        }
      }
      continue;
    }

    if (event.kind !== "acp_read" || method !== "session/update") {
      continue;
    }

    const params = asRecord(payload.params);
    const update = asRecord(params.update);
    const updateType = asString(update.sessionUpdate) ?? "unknown";
    const turnKey = event.turnId ?? event.sessionId ?? "unknown";
    const messageId = asString(update.messageId);

    if (updateType === "agent_message_chunk") {
      upsertMessage(
        `assistant:${messageId ?? turnKey}`,
        "assistant",
        "Assistant",
        extractContentText(update.content),
        event.timestamp,
      );
      continue;
    }

    if (updateType === "user_message_chunk") {
      upsertMessage(
        `user:${messageId ?? turnKey}`,
        "user",
        "User",
        extractContentText(update.content),
        event.timestamp,
      );
      continue;
    }

    if (updateType === "agent_thought_chunk") {
      upsertTextItem(
        `thinking:${messageId ?? turnKey}`,
        "thought",
        "Thinking",
        extractContentText(update.content),
        event.timestamp,
      );
      continue;
    }

    if (updateType === "tool_call") {
      const toolId = asString(update.toolCallId) ?? `tool:${event.seq}`;
      const identity = extractToolIdentity(update);
      upsertTool(
        `tool:${toolId}`,
        identity.title,
        identity.toolName,
        identity.sproutToolName,
        normalizeToolStatus(asString(update.status) ?? "executing"),
        extractToolArgs(update),
        extractToolResult(update),
        false,
        event.timestamp,
      );
      continue;
    }

    if (updateType === "tool_call_update") {
      const toolId = asString(update.toolCallId) ?? `tool:${event.seq}`;
      const status = normalizeToolStatus(
        asString(update.status) ?? "completed",
      );
      const identity = extractToolIdentity(update);
      upsertTool(
        `tool:${toolId}`,
        identity.title,
        identity.toolName,
        identity.sproutToolName,
        status,
        extractToolArgs(update),
        extractToolResult(update),
        status === "failed",
        event.timestamp,
      );
      continue;
    }

    if (updateType === "plan") {
      upsertTextItem(
        `plan:${turnKey}`,
        "thought",
        "Plan",
        extractContentText(update.content) || JSON.stringify(update, null, 2),
        event.timestamp,
      );
    }
  }

  return items;
}
