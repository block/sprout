import { Trash2 } from "lucide-react";

import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import {
  WorkflowStepActionFields,
  WorkflowStepActionSelect,
} from "./WorkflowEditorFields";
import { FieldLabel } from "./workflowFormPrimitives";
import type { StepFormState, TriggerType } from "./workflowFormTypes";

export function WorkflowStepCard({
  index,
  disabled,
  onRemove,
  onUpdate,
  step,
  triggerType,
}: {
  index: number;
  disabled?: boolean;
  onRemove: () => void;
  onUpdate: (step: StepFormState) => void;
  step: StepFormState;
  triggerType: TriggerType;
}) {
  const prefix = `wf-step-${index}`;

  return (
    <div className="space-y-3 rounded-lg border border-border/70 bg-muted/10 p-3">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium text-muted-foreground">
          Step {index + 1}
        </span>
        <Button
          aria-label="Remove step"
          className="h-7 w-7"
          disabled={disabled}
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
          <FieldLabel htmlFor={`${prefix}-id`}>Step ID</FieldLabel>
          <Input
            autoCapitalize="off"
            disabled={disabled}
            id={`${prefix}-id`}
            onChange={(event) => onUpdate({ ...step, id: event.target.value })}
            placeholder="unique_step_id"
            value={step.id}
          />
        </div>
        <div className="space-y-1.5">
          <FieldLabel htmlFor={`${prefix}-name`}>
            Step name (optional)
          </FieldLabel>
          <Input
            autoCapitalize="off"
            disabled={disabled}
            id={`${prefix}-name`}
            onChange={(event) =>
              onUpdate({ ...step, name: event.target.value })
            }
            placeholder="Human-friendly label"
            value={step.name ?? ""}
          />
        </div>
      </div>

      <div className="grid grid-cols-2 gap-2">
        <div className="space-y-1.5">
          <FieldLabel htmlFor={`${prefix}-action`}>Action</FieldLabel>
          <WorkflowStepActionSelect
            disabled={disabled}
            id={`${prefix}-action`}
            onChange={onUpdate}
            step={step}
          />
        </div>
        <div className="space-y-1.5">
          <FieldLabel htmlFor={`${prefix}-timeout-secs`}>
            Timeout seconds (optional)
          </FieldLabel>
          <Input
            autoCapitalize="off"
            disabled={disabled}
            id={`${prefix}-timeout-secs`}
            inputMode="numeric"
            onChange={(event) =>
              onUpdate({ ...step, timeoutSecs: event.target.value })
            }
            placeholder="e.g. 300"
            value={step.timeoutSecs ?? ""}
          />
        </div>
      </div>

      <div className="space-y-1.5">
        <FieldLabel htmlFor={`${prefix}-condition`}>
          Run condition (optional)
        </FieldLabel>
        <Input
          autoCapitalize="off"
          disabled={disabled}
          id={`${prefix}-condition`}
          onChange={(event) =>
            onUpdate({ ...step, condition: event.target.value })
          }
          placeholder='e.g. str_contains(trigger_text, "deploy")'
          value={step.condition ?? ""}
        />
      </div>

      <WorkflowStepActionFields
        disabled={disabled}
        onUpdate={onUpdate}
        prefix={prefix}
        step={step}
        triggerType={triggerType}
      />
    </div>
  );
}
