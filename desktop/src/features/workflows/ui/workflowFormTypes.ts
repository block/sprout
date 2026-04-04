import { stringify as yamlStringify, parse as yamlParse } from "yaml";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export const TRIGGER_TYPES = [
  "message_posted",
  "reaction_added",
  "webhook",
  "schedule",
] as const;
export type TriggerType = (typeof TRIGGER_TYPES)[number];

export const ACTION_TYPES = [
  "delay",
  "send_message",
  "send_dm",
  "call_webhook",
  "request_approval",
  "add_reaction",
  "set_channel_topic",
] as const;
export type ActionType = (typeof ACTION_TYPES)[number];

export type TriggerConfig = {
  on: TriggerType;
  filter?: string;
  emoji?: string;
  cron?: string;
  interval?: string;
};

export type StepFormState = {
  id: string;
  action: ActionType;
  duration?: string;
  text?: string;
  channel?: string;
  to?: string;
  url?: string;
  method?: string;
  body?: string;
  emoji?: string;
  topic?: string;
  from?: string;
  message?: string;
  timeout?: string;
};

export type WorkflowFormState = {
  name: string;
  trigger: TriggerConfig;
  steps: StepFormState[];
};

export const DEFAULT_FORM_STATE: WorkflowFormState = {
  name: "",
  trigger: { on: "message_posted" },
  steps: [],
};

export const TRIGGER_LABELS: Record<TriggerType, string> = {
  message_posted: "Message Posted",
  reaction_added: "Reaction Added",
  webhook: "Webhook",
  schedule: "Schedule",
};

export const ACTION_LABELS: Record<ActionType, string> = {
  delay: "Delay",
  send_message: "Send Message",
  send_dm: "Send DM",
  call_webhook: "Call Webhook",
  request_approval: "Request Approval",
  add_reaction: "Add Reaction",
  set_channel_topic: "Set Channel Topic",
};

// ---------------------------------------------------------------------------
// Serialization helpers
// ---------------------------------------------------------------------------

function actionFieldsForStep(step: StepFormState): Record<string, unknown> {
  const fields: Record<string, unknown> = {};
  switch (step.action) {
    case "delay":
      if (step.duration) fields.duration = step.duration;
      break;
    case "send_message":
      if (step.text) fields.text = step.text;
      if (step.channel) fields.channel = step.channel;
      break;
    case "send_dm":
      if (step.to) fields.to = step.to;
      if (step.text) fields.text = step.text;
      break;
    case "call_webhook":
      if (step.url) fields.url = step.url;
      fields.method = step.method || "POST";
      if (step.body) fields.body = step.body;
      break;
    case "request_approval":
      if (step.from) fields.from = step.from;
      if (step.message) fields.message = step.message;
      if (step.timeout) fields.timeout = step.timeout;
      break;
    case "add_reaction":
      if (step.emoji) fields.emoji = step.emoji;
      break;
    case "set_channel_topic":
      if (step.topic) fields.topic = step.topic;
      break;
  }
  return fields;
}

export function formStateToYaml(state: WorkflowFormState): string {
  const trigger: Record<string, unknown> = { on: state.trigger.on };
  if (state.trigger.on === "message_posted" && state.trigger.filter) {
    trigger.filter = state.trigger.filter;
  }
  if (state.trigger.on === "reaction_added" && state.trigger.emoji) {
    trigger.emoji = state.trigger.emoji;
  }
  if (state.trigger.on === "schedule") {
    if (state.trigger.cron) trigger.cron = state.trigger.cron;
    if (state.trigger.interval) trigger.interval = state.trigger.interval;
  }

  const steps = state.steps.map((step) => ({
    id: step.id,
    action: step.action,
    ...actionFieldsForStep(step),
  }));

  return yamlStringify({ name: state.name, trigger, steps });
}

const STEP_ID_PATTERN = /^step_(\d+)$/;

export function nextStepId(existingSteps: StepFormState[]): string {
  const existingIds = new Set(existingSteps.map((s) => s.id));
  let maxN = 0;
  for (const id of existingIds) {
    const match = STEP_ID_PATTERN.exec(id);
    if (match) maxN = Math.max(maxN, Number(match[1]));
  }
  let n = maxN + 1;
  while (existingIds.has(`step_${n}`)) n++;
  return `step_${n}`;
}

export function yamlToFormState(
  yaml: string,
): { ok: true; state: WorkflowFormState } | { ok: false; error: string } {
  try {
    const parsed = yamlParse(yaml);
    if (!parsed || typeof parsed !== "object") {
      return { ok: false, error: "YAML must be an object" };
    }

    const triggerOn = parsed.trigger?.on;
    if (triggerOn && !TRIGGER_TYPES.includes(triggerOn as TriggerType)) {
      return {
        ok: false,
        error: `Unsupported trigger type "${triggerOn}" — use the YAML editor`,
      };
    }
    const trigger: TriggerConfig = {
      on: (triggerOn as TriggerType) ?? TRIGGER_TYPES[0],
      filter: parsed.trigger?.filter ?? undefined,
      emoji: parsed.trigger?.emoji ?? undefined,
      cron: parsed.trigger?.cron ?? undefined,
      interval: parsed.trigger?.interval ?? undefined,
    };

    const rawSteps = parsed.steps ?? [];
    if (!Array.isArray(rawSteps)) {
      return { ok: false, error: "steps must be a list" };
    }

    for (const step of rawSteps) {
      if (step.action && !ACTION_TYPES.includes(step.action as ActionType)) {
        return {
          ok: false,
          error: `Unsupported action type "${step.action}" — use the YAML editor`,
        };
      }
    }

    const steps: StepFormState[] = rawSteps.map(
      (step: Record<string, unknown>, index: number) => ({
        id: (step.id as string) ?? `step_${index + 1}`,
        action: (step.action as ActionType) ?? ACTION_TYPES[0],
        duration: step.duration as string | undefined,
        text: step.text as string | undefined,
        channel: step.channel as string | undefined,
        to: step.to as string | undefined,
        url: step.url as string | undefined,
        method: step.method as string | undefined,
        body: step.body as string | undefined,
        emoji: step.emoji as string | undefined,
        topic: step.topic as string | undefined,
        from: step.from as string | undefined,
        message: step.message as string | undefined,
        timeout: step.timeout as string | undefined,
      }),
    );

    return {
      ok: true,
      state: { name: (parsed.name as string) ?? "", trigger, steps },
    };
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "Invalid YAML",
    };
  }
}
