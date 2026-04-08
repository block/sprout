import type {
  StepFormState,
  TriggerConfig,
  WorkflowFormState,
} from "./workflowFormTypes";
import { formStateToYaml } from "./workflowFormTypes";

const URL_PATTERN = /https:\/\/[^\s"')]+/i;
const QUOTED_TEXT_PATTERN = /"([^"\n]+)"|'([^'\n]+)'/g;
const EMOJI_PATTERN = /:([a-z0-9_+-]+):/i;
const EVERY_INTERVAL_PATTERN =
  /\bevery\s+(\d+)\s*(minute|minutes|hour|hours|day|days|week|weeks)\b/i;
const WAIT_INTERVAL_PATTERN =
  /\b(?:wait|delay)\s+(?:for\s+)?(\d+)\s*(second|seconds|minute|minutes|hour|hours|day|days)\b/i;

function toSnakeCase(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .replace(/_+/g, "_");
}

function collectQuotedTexts(prompt: string): string[] {
  const matches: string[] = [];
  for (const match of prompt.matchAll(QUOTED_TEXT_PATTERN)) {
    const value = match[1] ?? match[2] ?? "";
    if (value.trim()) {
      matches.push(value.trim());
    }
  }
  return matches;
}

function firstNonEmptyMatch(match: RegExpMatchArray | null, index: number) {
  const value = match?.[index]?.trim();
  return value ? value : null;
}

function normalizeInterval(value: string, unit: string): string {
  const trimmedUnit = unit.toLowerCase();
  if (trimmedUnit.startsWith("second")) return `${value}s`;
  if (trimmedUnit.startsWith("minute")) return `${value}m`;
  if (trimmedUnit.startsWith("hour")) return `${value}h`;
  if (trimmedUnit.startsWith("day")) return `${value}d`;
  if (trimmedUnit.startsWith("week")) return `${Number(value) * 7}d`;
  return `${value}h`;
}

function inferScheduleInterval(prompt: string): string | undefined {
  const everyMatch = prompt.match(EVERY_INTERVAL_PATTERN);
  if (everyMatch) {
    return normalizeInterval(everyMatch[1], everyMatch[2]);
  }

  const lower = prompt.toLowerCase();
  if (lower.includes("hourly") || lower.includes("every hour")) {
    return "1h";
  }
  if (lower.includes("daily") || lower.includes("every day")) {
    return "1d";
  }
  if (lower.includes("weekly") || lower.includes("every week")) {
    return "7d";
  }

  return undefined;
}

function inferDelayDuration(prompt: string): string | undefined {
  const waitMatch = prompt.match(WAIT_INTERVAL_PATTERN);
  if (waitMatch) {
    return normalizeInterval(waitMatch[1], waitMatch[2]);
  }
  return undefined;
}

function inferEmoji(prompt: string): string | undefined {
  const explicitEmoji = firstNonEmptyMatch(prompt.match(EMOJI_PATTERN), 1);
  if (explicitEmoji) {
    return explicitEmoji;
  }

  const lower = prompt.toLowerCase();
  const namedEmojiMatches: Array<[string, string]> = [
    ["thumbs up", "thumbsup"],
    ["thumbsup", "thumbsup"],
    ["thumbsdown", "thumbsdown"],
    ["eyes", "eyes"],
    ["rocket", "rocket"],
    ["check", "white_check_mark"],
  ];

  for (const [needle, emoji] of namedEmojiMatches) {
    if (lower.includes(needle)) {
      return emoji;
    }
  }

  return undefined;
}

function inferTopic(prompt: string): string | undefined {
  const quotedTexts = collectQuotedTexts(prompt);
  if (quotedTexts.length > 0) {
    return quotedTexts[0];
  }

  const match = prompt.match(/\btopic(?: to| as)?\s+([a-z0-9 _-]+)/i);
  return firstNonEmptyMatch(match, 1) ?? undefined;
}

function inferFilterTerm(prompt: string): string | undefined {
  const quotedTexts = collectQuotedTexts(prompt);
  if (quotedTexts.length > 0) {
    return quotedTexts[0];
  }

  const simpleMessageMatch = prompt.match(
    /\b(?:posts?|says?|contains?|mentions?)\s+([a-z0-9._/-]+)/i,
  );
  return firstNonEmptyMatch(simpleMessageMatch, 1) ?? undefined;
}

function asContainsFilter(term: string | undefined): string | undefined {
  if (!term) return undefined;
  const normalized = term.replace(/\\/g, "\\\\").replace(/"/g, '\\"').trim();
  return normalized ? `contains(text, "${normalized}")` : undefined;
}

function inferTrigger(prompt: string): TriggerConfig {
  const lower = prompt.toLowerCase();
  const filter = asContainsFilter(inferFilterTerm(prompt));

  const incomingWebhook =
    /\b(incoming webhook|when a webhook|when webhook|webhook trigger)\b/i.test(
      prompt,
    ) || /\breceive\b.*\bwebhook\b/i.test(prompt);
  if (incomingWebhook) {
    return { on: "webhook" };
  }

  const looksScheduled =
    /\b(schedule|scheduled|every|hourly|daily|weekly|cron)\b/i.test(prompt);
  if (looksScheduled) {
    return {
      on: "schedule",
      interval: inferScheduleInterval(prompt),
    };
  }

  if (/\b(reaction|emoji)\b/i.test(prompt)) {
    return {
      on: "reaction_added",
      emoji: inferEmoji(prompt),
    };
  }

  if (/\b(diff|pull request|pr\b|patch)\b/i.test(lower)) {
    return {
      on: "diff_posted",
      filter,
    };
  }

  return {
    on: "message_posted",
    filter,
  };
}

const INFERRED_TITLE_MAX = 80;

/**
 * Human-readable workflow title for YAML `name` (shown in the app header).
 * Prefers the first line of the user’s prompt; falls back to short template labels.
 */
function inferWorkflowName(prompt: string, trigger: TriggerConfig): string {
  const lower = prompt.toLowerCase();
  const firstLine = prompt.trim().split(/\r?\n/)[0]?.trim() ?? "";

  if (trigger.on === "webhook") return "Incoming webhook";
  if (trigger.on === "schedule") return "Scheduled workflow";
  if (trigger.on === "reaction_added") return "Reaction workflow";
  if (trigger.on === "diff_posted") return "Diff review";

  if (lower.includes("deploy")) return "Deploy notifications";
  if (lower.includes("alert")) return "Alert workflow";
  if (lower.includes("remind")) return "Reminder workflow";
  if (lower.includes("notify")) return "Notification workflow";

  if (firstLine.length > 0) {
    let title = firstLine.replace(/\s+/g, " ");
    if (title.length > INFERRED_TITLE_MAX) {
      title = `${title.slice(0, INFERRED_TITLE_MAX - 1).trimEnd()}…`;
    }
    return title.charAt(0).toUpperCase() + title.slice(1);
  }

  const fallback = toSnakeCase(prompt).slice(0, 48);
  return fallback ? humanizeSnakeFallback(fallback) : "New workflow";
}

/** Last resort when prompt yields only a slug — still nicer than raw snake_case in UI. */
function humanizeSnakeFallback(slug: string): string {
  return slug
    .split("_")
    .filter(Boolean)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase())
    .join(" ");
}

function buildMessageStep(
  trigger: TriggerConfig,
  prompt: string,
  defaultChannelId?: string,
): StepFormState {
  const filterTerm = inferFilterTerm(prompt);

  let text = "Workflow triggered.";
  if (trigger.on === "webhook") {
    text = "Webhook received and processed.";
  } else if (trigger.on === "schedule") {
    text = "Scheduled workflow ran.";
  } else if (trigger.on === "reaction_added") {
    text = trigger.emoji
      ? `Reaction :${trigger.emoji}: detected.`
      : "A reaction triggered this workflow.";
  } else if (trigger.on === "diff_posted") {
    text = filterTerm
      ? `Diff matched "${filterTerm}".`
      : "A diff triggered this workflow.";
  } else if (filterTerm) {
    text = `Detected "${filterTerm}" in a message.`;
  } else {
    text = "A new message triggered this workflow.";
  }

  return {
    id: "step_1",
    name: "Notify channel",
    action: "send_message",
    channel: trigger.on === "webhook" ? defaultChannelId : undefined,
    text,
  };
}

function buildPrimaryActionStep(
  prompt: string,
  trigger: TriggerConfig,
  defaultChannelId?: string,
): StepFormState {
  const lower = prompt.toLowerCase();
  const url = firstNonEmptyMatch(prompt.match(URL_PATTERN), 0);

  if (
    /\b(call|post|send|forward|hit|invoke)\b.*\bwebhook\b/i.test(prompt) ||
    (url !== null && trigger.on !== "webhook")
  ) {
    return {
      id: "step_1",
      name: "Call webhook",
      action: "call_webhook",
      url: url ?? "https://example.com/webhook",
      method: "POST",
      body: '{"text":"{{trigger.text}}"}',
    };
  }

  if (/\b(direct message| dm |^dm\b|\bdm\b)\b/i.test(` ${lower} `)) {
    return {
      id: "step_1",
      name: "Send direct message",
      action: "send_dm",
      to: "{{trigger.author}}",
      text: "Workflow triggered.",
    };
  }

  if (/\b(approve|approval)\b/i.test(prompt)) {
    return {
      id: "step_1",
      name: "Request approval",
      action: "request_approval",
      from: "channel_admins",
      message: "Approve this workflow run.",
      timeout: "24h",
    };
  }

  if (
    /\b(add reaction|react with|react using|leave a reaction)\b/i.test(prompt)
  ) {
    return {
      id: "step_1",
      name: "Add reaction",
      action: "add_reaction",
      emoji: inferEmoji(prompt) ?? "eyes",
    };
  }

  if (/\b(set|change|update)\b.*\btopic\b/i.test(prompt)) {
    return {
      id: "step_1",
      name: "Set channel topic",
      action: "set_channel_topic",
      topic: inferTopic(prompt) ?? "Workflow updated topic",
    };
  }

  return buildMessageStep(trigger, prompt, defaultChannelId);
}

function buildSteps(
  prompt: string,
  trigger: TriggerConfig,
  defaultChannelId?: string,
): StepFormState[] {
  const delayDuration = inferDelayDuration(prompt);
  const primary = buildPrimaryActionStep(prompt, trigger, defaultChannelId);

  if (!delayDuration) {
    return [primary];
  }

  return [
    {
      id: "step_1",
      name: "Wait",
      action: "delay",
      duration: delayDuration,
    },
    {
      ...primary,
      id: "step_2",
    },
  ];
}

export function draftWorkflowFromPrompt(
  prompt: string,
  options?: { defaultChannelId?: string },
): WorkflowFormState {
  const trimmedPrompt = prompt.trim();
  const trigger = inferTrigger(trimmedPrompt);

  return {
    name: inferWorkflowName(trimmedPrompt, trigger),
    description: trimmedPrompt,
    enabled: true,
    trigger,
    steps: buildSteps(trimmedPrompt, trigger, options?.defaultChannelId),
  };
}

export function draftWorkflowYamlFromPrompt(
  prompt: string,
  options?: { defaultChannelId?: string },
): string {
  return formStateToYaml(draftWorkflowFromPrompt(prompt, options));
}
