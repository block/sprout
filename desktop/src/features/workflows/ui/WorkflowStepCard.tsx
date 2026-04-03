import { ChevronDown, Trash2 } from "lucide-react";
import type * as React from "react";

import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { Textarea } from "@/shared/ui/textarea";
import { ACTION_LABELS, ACTION_TYPES } from "./workflowFormTypes";
import type { ActionType, StepFormState } from "./workflowFormTypes";

// ---------------------------------------------------------------------------
// Shared primitives
// ---------------------------------------------------------------------------

function FormSelect({
  children,
  onChange,
  value,
}: {
  children: React.ReactNode;
  onChange: (value: string) => void;
  value: string;
}) {
  return (
    <div className="relative">
      <select
        className="flex h-9 w-full appearance-none rounded-md border border-input bg-transparent px-3 pr-8 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
        onChange={(event) => onChange(event.target.value)}
        value={value}
      >
        {children}
      </select>
      <ChevronDown className="pointer-events-none absolute right-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
    </div>
  );
}

function FieldLabel({ children }: { children: React.ReactNode }) {
  return (
    <span className="block text-xs font-medium text-muted-foreground">
      {children}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Step action config fields
// ---------------------------------------------------------------------------

function StepConfigFields({
  step,
  onUpdate,
}: {
  step: StepFormState;
  onUpdate: (step: StepFormState) => void;
}) {
  switch (step.action) {
    case "delay":
      return (
        <div className="space-y-1.5">
          <FieldLabel>Duration</FieldLabel>
          <Input
            onChange={(event) =>
              onUpdate({ ...step, duration: event.target.value })
            }
            placeholder="e.g. 5s, 1m, 1h"
            value={step.duration ?? ""}
          />
        </div>
      );
    case "send_message":
      return (
        <div className="space-y-2">
          <div className="space-y-1.5">
            <FieldLabel>Message text</FieldLabel>
            <Textarea
              className="min-h-[60px] resize-y text-xs"
              onChange={(event) =>
                onUpdate({ ...step, text: event.target.value })
              }
              placeholder="e.g. Deployment started by {{trigger.author}}"
              value={step.text ?? ""}
            />
          </div>
          <div className="space-y-1.5">
            <FieldLabel>Channel override (optional)</FieldLabel>
            <Input
              onChange={(event) =>
                onUpdate({ ...step, channel: event.target.value })
              }
              placeholder="Channel UUID — leave empty for trigger channel"
              value={step.channel ?? ""}
            />
          </div>
        </div>
      );
    case "send_dm":
      return (
        <div className="space-y-2">
          <div className="space-y-1.5">
            <FieldLabel>To (pubkey)</FieldLabel>
            <Input
              onChange={(event) =>
                onUpdate({ ...step, to: event.target.value })
              }
              placeholder="e.g. {{trigger.author}} or hex pubkey"
              value={step.to ?? ""}
            />
          </div>
          <div className="space-y-1.5">
            <FieldLabel>Message text</FieldLabel>
            <Textarea
              className="min-h-[60px] resize-y text-xs"
              onChange={(event) =>
                onUpdate({ ...step, text: event.target.value })
              }
              placeholder="DM content"
              value={step.text ?? ""}
            />
          </div>
        </div>
      );
    case "call_webhook":
      return (
        <div className="space-y-2">
          <div className="space-y-1.5">
            <FieldLabel>URL</FieldLabel>
            <Input
              onChange={(event) =>
                onUpdate({ ...step, url: event.target.value })
              }
              placeholder="https://..."
              value={step.url ?? ""}
            />
          </div>
          <div className="space-y-1.5">
            <FieldLabel>Method (optional)</FieldLabel>
            <FormSelect
              onChange={(value) => onUpdate({ ...step, method: value })}
              value={step.method ?? "POST"}
            >
              <option value="POST">POST</option>
              <option value="GET">GET</option>
              <option value="PUT">PUT</option>
              <option value="PATCH">PATCH</option>
              <option value="DELETE">DELETE</option>
            </FormSelect>
          </div>
          <div className="space-y-1.5">
            <FieldLabel>Body (optional)</FieldLabel>
            <Textarea
              className="min-h-[60px] resize-y font-mono text-xs"
              onChange={(event) =>
                onUpdate({ ...step, body: event.target.value })
              }
              placeholder='{"key": "{{trigger.text}}"}'
              value={step.body ?? ""}
            />
          </div>
        </div>
      );
    case "request_approval":
      return (
        <div className="space-y-2">
          <div className="space-y-1.5">
            <FieldLabel>From (approver)</FieldLabel>
            <Input
              onChange={(event) =>
                onUpdate({ ...step, from: event.target.value })
              }
              placeholder="Pubkey or role"
              value={step.from ?? ""}
            />
          </div>
          <div className="space-y-1.5">
            <FieldLabel>Message</FieldLabel>
            <Input
              onChange={(event) =>
                onUpdate({ ...step, message: event.target.value })
              }
              placeholder="Approval request message"
              value={step.message ?? ""}
            />
          </div>
          <div className="space-y-1.5">
            <FieldLabel>Timeout (optional)</FieldLabel>
            <Input
              onChange={(event) =>
                onUpdate({ ...step, timeout: event.target.value })
              }
              placeholder="e.g. 24h"
              value={step.timeout ?? ""}
            />
          </div>
        </div>
      );
    case "add_reaction":
      return (
        <div className="space-y-1.5">
          <FieldLabel>Emoji</FieldLabel>
          <Input
            onChange={(event) =>
              onUpdate({ ...step, emoji: event.target.value })
            }
            placeholder="e.g. thumbsup"
            value={step.emoji ?? ""}
          />
        </div>
      );
    case "set_channel_topic":
      return (
        <div className="space-y-1.5">
          <FieldLabel>Topic</FieldLabel>
          <Input
            onChange={(event) =>
              onUpdate({ ...step, topic: event.target.value })
            }
            placeholder="New channel topic"
            value={step.topic ?? ""}
          />
        </div>
      );
    default:
      return null;
  }
}

// ---------------------------------------------------------------------------
// Step card
// ---------------------------------------------------------------------------

export function WorkflowStepCard({
  index,
  onRemove,
  onUpdate,
  step,
}: {
  index: number;
  onRemove: () => void;
  onUpdate: (step: StepFormState) => void;
  step: StepFormState;
}) {
  return (
    <div className="space-y-3 rounded-lg border border-border/70 bg-muted/10 p-3">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium text-muted-foreground">
          Step {index + 1}
        </span>
        <Button
          aria-label="Remove step"
          className="h-7 w-7"
          onClick={onRemove}
          size="icon"
          type="button"
          variant="ghost"
        >
          <Trash2 className="h-3.5 w-3.5 text-muted-foreground" />
        </Button>
      </div>

      <div className="grid grid-cols-2 gap-2">
        <div className="space-y-1.5">
          <FieldLabel>Step ID</FieldLabel>
          <Input
            onChange={(event) => onUpdate({ ...step, id: event.target.value })}
            placeholder="unique_step_id"
            value={step.id}
          />
        </div>
        <div className="space-y-1.5">
          <FieldLabel>Action</FieldLabel>
          <FormSelect
            onChange={(value) =>
              onUpdate({ ...step, action: value as ActionType })
            }
            value={step.action}
          >
            {ACTION_TYPES.map((action) => (
              <option key={action} value={action}>
                {ACTION_LABELS[action]}
              </option>
            ))}
          </FormSelect>
        </div>
      </div>

      <StepConfigFields onUpdate={onUpdate} step={step} />
    </div>
  );
}
