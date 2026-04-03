import { Trash2 } from "lucide-react";

import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { Textarea } from "@/shared/ui/textarea";
import { FieldLabel, FormSelect } from "./workflowFormPrimitives";
import { ACTION_LABELS, ACTION_TYPES } from "./workflowFormTypes";
import type { ActionType, StepFormState } from "./workflowFormTypes";

// ---------------------------------------------------------------------------
// Step action config fields
// ---------------------------------------------------------------------------

function StepConfigFields({
  step,
  index,
  onUpdate,
}: {
  step: StepFormState;
  index: number;
  onUpdate: (step: StepFormState) => void;
}) {
  const pfx = `wf-step-${index}`;
  switch (step.action) {
    case "delay":
      return (
        <div className="space-y-1.5">
          <FieldLabel htmlFor={`${pfx}-duration`}>Duration</FieldLabel>
          <Input
            id={`${pfx}-duration`}
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
            <FieldLabel htmlFor={`${pfx}-text`}>Message text</FieldLabel>
            <Textarea
              className="min-h-[60px] resize-y text-xs"
              id={`${pfx}-text`}
              onChange={(event) =>
                onUpdate({ ...step, text: event.target.value })
              }
              placeholder="e.g. Deployment started by {{trigger.author}}"
              value={step.text ?? ""}
            />
          </div>
          <div className="space-y-1.5">
            <FieldLabel htmlFor={`${pfx}-channel`}>
              Channel override (optional)
            </FieldLabel>
            <Input
              id={`${pfx}-channel`}
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
            <FieldLabel htmlFor={`${pfx}-to`}>To (pubkey)</FieldLabel>
            <Input
              id={`${pfx}-to`}
              onChange={(event) =>
                onUpdate({ ...step, to: event.target.value })
              }
              placeholder="e.g. {{trigger.author}} or hex pubkey"
              value={step.to ?? ""}
            />
          </div>
          <div className="space-y-1.5">
            <FieldLabel htmlFor={`${pfx}-text`}>Message text</FieldLabel>
            <Textarea
              className="min-h-[60px] resize-y text-xs"
              id={`${pfx}-text`}
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
            <FieldLabel htmlFor={`${pfx}-url`}>URL</FieldLabel>
            <Input
              id={`${pfx}-url`}
              onChange={(event) =>
                onUpdate({ ...step, url: event.target.value })
              }
              placeholder="https://..."
              value={step.url ?? ""}
            />
            {step.url && !step.url.startsWith("https://") ? (
              <p className="text-xs text-destructive">
                URL must start with https://
              </p>
            ) : null}
          </div>
          <div className="space-y-1.5">
            <FieldLabel htmlFor={`${pfx}-method`}>Method (optional)</FieldLabel>
            <FormSelect
              id={`${pfx}-method`}
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
            <FieldLabel htmlFor={`${pfx}-body`}>Body (optional)</FieldLabel>
            <Textarea
              className="min-h-[60px] resize-y font-mono text-xs"
              id={`${pfx}-body`}
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
            <FieldLabel htmlFor={`${pfx}-from`}>From (approver)</FieldLabel>
            <Input
              id={`${pfx}-from`}
              onChange={(event) =>
                onUpdate({ ...step, from: event.target.value })
              }
              placeholder="Pubkey or role"
              value={step.from ?? ""}
            />
          </div>
          <div className="space-y-1.5">
            <FieldLabel htmlFor={`${pfx}-message`}>Message</FieldLabel>
            <Input
              id={`${pfx}-message`}
              onChange={(event) =>
                onUpdate({ ...step, message: event.target.value })
              }
              placeholder="Approval request message"
              value={step.message ?? ""}
            />
          </div>
          <div className="space-y-1.5">
            <FieldLabel htmlFor={`${pfx}-timeout`}>
              Timeout (optional)
            </FieldLabel>
            <Input
              id={`${pfx}-timeout`}
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
          <FieldLabel htmlFor={`${pfx}-emoji`}>Emoji</FieldLabel>
          <Input
            id={`${pfx}-emoji`}
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
          <FieldLabel htmlFor={`${pfx}-topic`}>Topic</FieldLabel>
          <Input
            id={`${pfx}-topic`}
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
          <FieldLabel htmlFor={`wf-step-${index}-id`}>Step ID</FieldLabel>
          <Input
            id={`wf-step-${index}-id`}
            onChange={(event) => onUpdate({ ...step, id: event.target.value })}
            placeholder="unique_step_id"
            value={step.id}
          />
        </div>
        <div className="space-y-1.5">
          <FieldLabel htmlFor={`wf-step-${index}-action`}>Action</FieldLabel>
          <FormSelect
            id={`wf-step-${index}-action`}
            onChange={(value) => {
              const next = { ...step, action: value as ActionType };
              if (value === "call_webhook" && !next.method) {
                next.method = "POST";
              }
              onUpdate(next);
            }}
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

      <StepConfigFields index={index} onUpdate={onUpdate} step={step} />
    </div>
  );
}
