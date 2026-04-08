import {
  ACTION_LABELS,
  TRIGGER_LABELS,
  type ActionType,
  type TriggerType,
} from "./workflowFormTypes";

export function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

export function asString(value: unknown): string | null {
  return typeof value === "string" && value.trim().length > 0
    ? value.trim()
    : null;
}

function truncateText(value: string | null, maxLength = 72): string | null {
  if (!value) return null;
  return value.length > maxLength
    ? `${value.slice(0, maxLength - 1)}...`
    : value;
}

export function getTriggerTitle(
  trigger: Record<string, unknown> | null,
): string {
  const on = asString(trigger?.on);
  if (!on) return "Trigger";
  return TRIGGER_LABELS[on as TriggerType] ?? on;
}

export function getTriggerDetails(
  trigger: Record<string, unknown> | null,
): string[] {
  if (!trigger) return [];

  const on = asString(trigger.on);
  const details: string[] = [];

  if (on === "message_posted" || on === "diff_posted") {
    const filter = asString(trigger.filter);
    if (filter) details.push(`Filter: ${truncateText(filter)}`);
  }

  if (on === "reaction_added") {
    const emoji = asString(trigger.emoji);
    if (emoji) details.push(`Emoji: :${emoji}:`);
  }

  if (on === "schedule") {
    const cron = asString(trigger.cron);
    const interval = asString(trigger.interval);
    if (cron) details.push(`Cron: ${truncateText(cron)}`);
    if (interval) details.push(`Interval: ${interval}`);
  }

  if (on === "webhook") {
    details.push("Incoming HTTP webhook");
  }

  return details;
}

export function getStepTitle(
  step: Record<string, unknown>,
  index: number,
): string {
  const name = asString(step.name);
  if (name) return name;

  const action = asString(step.action);
  if (action) {
    return ACTION_LABELS[action as ActionType] ?? action;
  }

  return `Step ${index + 1}`;
}

export function getStepDetails(step: Record<string, unknown>): string[] {
  const details: string[] = [];
  const action = asString(step.action);
  const condition = asString(step.if);
  const timeoutSecs = asString(step.timeout_secs);

  if (condition) details.push(`If: ${truncateText(condition)}`);
  if (timeoutSecs) details.push(`Timeout: ${timeoutSecs}s`);

  switch (action) {
    case "delay": {
      const duration = asString(step.duration);
      if (duration) details.push(`Wait: ${duration}`);
      break;
    }
    case "send_message": {
      const text = truncateText(asString(step.text));
      const channel = asString(step.channel);
      if (text) details.push(`Message: ${text}`);
      if (channel) details.push(`Channel: ${channel}`);
      break;
    }
    case "send_dm": {
      const to = asString(step.to);
      const text = truncateText(asString(step.text));
      if (to) details.push(`To: ${to}`);
      if (text) details.push(`Message: ${text}`);
      break;
    }
    case "call_webhook": {
      const method = asString(step.method) ?? "POST";
      const url = truncateText(asString(step.url));
      details.push(`Method: ${method}`);
      if (url) details.push(`URL: ${url}`);
      break;
    }
    case "request_approval": {
      const from = asString(step.from);
      const message = truncateText(asString(step.message));
      const timeout = asString(step.timeout);
      if (from) details.push(`Approver: ${from}`);
      if (message) details.push(`Message: ${message}`);
      if (timeout) details.push(`Timeout: ${timeout}`);
      break;
    }
    case "add_reaction": {
      const emoji = asString(step.emoji);
      if (emoji) details.push(`Emoji: :${emoji}:`);
      break;
    }
    case "set_channel_topic": {
      const topic = truncateText(asString(step.topic));
      if (topic) details.push(`Topic: ${topic}`);
      break;
    }
    default:
      break;
  }

  return details;
}

export type ResolvedGraphSelection =
  | { kind: "none" }
  | { kind: "trigger"; trigger: Record<string, unknown> | null }
  | {
      kind: "step";
      index: number;
      step: Record<string, unknown>;
      stepId: string;
    };

/** Graph node ids are `trigger` or `step-${stepId}` (same ids as React Flow nodes). */
export function resolveGraphNodeSelection(
  definition: Record<string, unknown>,
  selectedNodeId: string | null,
): ResolvedGraphSelection {
  if (!selectedNodeId) {
    return { kind: "none" };
  }
  if (selectedNodeId === "trigger") {
    return { kind: "trigger", trigger: asRecord(definition.trigger) };
  }
  if (selectedNodeId.startsWith("step-")) {
    const targetStepId = selectedNodeId.slice("step-".length);
    const rawSteps = Array.isArray(definition.steps) ? definition.steps : [];
    for (let i = 0; i < rawSteps.length; i++) {
      const step = asRecord(rawSteps[i]);
      if (!step) continue;
      const sid = asString(step.id) ?? `step_${i + 1}`;
      if (sid === targetStepId) {
        return { kind: "step", index: i, step, stepId: sid };
      }
    }
  }
  return { kind: "none" };
}
