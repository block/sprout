import type { Workflow } from "@/shared/api/types";
import { TRIGGER_LABELS } from "./workflowFormTypes";
import type { TriggerType } from "./workflowFormTypes";

const DISPLAY_TITLE_MAX = 100;

/** Stable name from YAML / API (may be snake_case). */
export function getWorkflowDefinitionName(workflow: Workflow): string {
  const fromDef = workflow.definition?.name;
  if (typeof fromDef === "string" && fromDef.trim().length > 0) {
    return fromDef.trim();
  }
  return workflow.name.trim();
}

/**
 * Turns stored workflow names into a readable header/card title.
 * Pass-through for names that already look like sentences or titles; otherwise
 * converts snake_case to Title Case words.
 */
export function humanizeWorkflowTitle(raw: string): string {
  const name = raw.trim();
  if (!name) {
    return "Untitled workflow";
  }
  if (name.length > DISPLAY_TITLE_MAX) {
    return `${name.slice(0, DISPLAY_TITLE_MAX - 1).trimEnd()}…`;
  }

  if (name.includes(" ") || !name.includes("_")) {
    return name;
  }

  return name
    .split("_")
    .filter(Boolean)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase())
    .join(" ");
}

/** Title shown in headers, cards, and dialogs. */
export function getWorkflowDisplayTitle(workflow: Workflow): string {
  return humanizeWorkflowTitle(getWorkflowDefinitionName(workflow));
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

export function getWorkflowEnabled(
  definition: Record<string, unknown>,
): boolean {
  return definition.enabled !== false;
}

export function getWorkflowDisplayStatus(
  workflow: Workflow,
): Workflow["status"] | "disabled" {
  if (workflow.status !== "active") {
    return workflow.status;
  }

  return getWorkflowEnabled(workflow.definition) ? workflow.status : "disabled";
}

export function getWorkflowDescription(
  definition: Record<string, unknown>,
): string | null {
  const description = definition.description;
  return typeof description === "string" && description.trim().length > 0
    ? description.trim()
    : null;
}

export function getWorkflowTriggerSummary(
  definition: Record<string, unknown>,
): string | null {
  const trigger = asRecord(definition.trigger);
  if (!trigger) return null;

  const on = trigger.on;
  if (typeof on !== "string") return null;

  const label = TRIGGER_LABELS[on as TriggerType] ?? on;
  switch (on) {
    case "message_posted":
    case "diff_posted":
      return typeof trigger.filter === "string" &&
        trigger.filter.trim().length > 0
        ? `${label} · ${trigger.filter}`
        : label;
    case "reaction_added":
      return typeof trigger.emoji === "string" &&
        trigger.emoji.trim().length > 0
        ? `${label} · ${trigger.emoji}`
        : label;
    case "schedule":
      if (typeof trigger.cron === "string" && trigger.cron.trim().length > 0) {
        return `${label} · ${trigger.cron}`;
      }
      if (
        typeof trigger.interval === "string" &&
        trigger.interval.trim().length > 0
      ) {
        return `${label} · ${trigger.interval}`;
      }
      return label;
    default:
      return label;
  }
}
