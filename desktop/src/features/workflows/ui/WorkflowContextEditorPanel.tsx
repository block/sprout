import { Save } from "lucide-react";
import * as React from "react";

import { Button } from "@/shared/ui/button";
import { Checkbox } from "@/shared/ui/checkbox";
import { Input } from "@/shared/ui/input";
import { Textarea } from "@/shared/ui/textarea";
import { cn } from "@/shared/lib/cn";
import {
  OptionalFieldToggle,
  TriggerConfigFields,
  WorkflowStepActionFields,
  WorkflowStepActionSelect,
} from "./WorkflowEditorFields";
import { FieldLabel, FormSelect } from "./workflowFormPrimitives";
import {
  TRIGGER_LABELS,
  TRIGGER_TYPES,
  type StepFormState,
  type TriggerType,
  type WorkflowFormState,
} from "./workflowFormTypes";

type WorkflowContextEditorPanelProps = {
  channelName?: string | null;
  className?: string;
  disabled?: boolean;
  errorMessage?: string | null;
  formState: WorkflowFormState;
  isSaving?: boolean;
  selectedNodeId: string | null;
  onChange: (next: WorkflowFormState) => void;
  onSave: () => void;
  onSelectedNodeIdChange?: (nodeId: string | null) => void;
};

function resolveSelectedStep(
  steps: StepFormState[],
  selectedNodeId: string | null,
): { index: number; step: StepFormState } | null {
  if (!selectedNodeId?.startsWith("step-")) {
    return null;
  }

  const stepId = selectedNodeId.slice("step-".length);
  const index = steps.findIndex((step, stepIndex) => {
    const effectiveId = step.id.trim() || `step_${stepIndex + 1}`;
    return effectiveId === stepId;
  });

  if (index < 0) {
    return null;
  }

  return { index, step: steps[index] };
}

export function WorkflowContextEditorPanel({
  channelName,
  className,
  disabled,
  errorMessage,
  formState,
  isSaving,
  selectedNodeId,
  onChange,
  onSave,
  onSelectedNodeIdChange,
}: WorkflowContextEditorPanelProps) {
  const selectedStep = resolveSelectedStep(formState.steps, selectedNodeId);
  const isTriggerSelected = selectedNodeId === "trigger";
  const [showDescriptionField, setShowDescriptionField] = React.useState(
    Boolean(formState.description.trim()),
  );

  React.useEffect(() => {
    if (formState.description.trim()) {
      setShowDescriptionField(true);
    }
  }, [formState.description]);

  const updateWorkflow = (patch: Partial<WorkflowFormState>) => {
    onChange({ ...formState, ...patch });
  };

  const updateStep = (index: number, step: StepFormState) => {
    const nextSteps = [...formState.steps];
    nextSteps[index] = step;
    onChange({ ...formState, steps: nextSteps });
  };

  return (
    <div
      className={cn("flex flex-col gap-4 p-4", className)}
      data-testid="workflow-context-editor-panel"
    >
      {selectedStep ? (
        <>
          <h3 className="text-sm font-semibold">Step {selectedStep.index + 1}</h3>

          <div className="space-y-1.5">
            <FieldLabel htmlFor="wf-context-step-id">Step ID</FieldLabel>
            <Input
              autoCapitalize="off"
              disabled={disabled}
              id="wf-context-step-id"
              onChange={(event) => {
                const nextId = event.target.value;
                updateStep(selectedStep.index, {
                  ...selectedStep.step,
                  id: nextId,
                });
                onSelectedNodeIdChange?.(
                  `step-${nextId.trim() || `step_${selectedStep.index + 1}`}`,
                );
              }}
              placeholder="unique_step_id"
              value={selectedStep.step.id}
            />
          </div>

          <OptionalFieldToggle
            checked={selectedStep.step.name !== undefined}
            disabled={disabled}
            id="wf-context-step-name-toggle"
            label="Step name"
            onCheckedChange={(checked) =>
              updateStep(selectedStep.index, {
                ...selectedStep.step,
                name: checked ? (selectedStep.step.name ?? "") : undefined,
              })
            }
          >
            <FieldLabel htmlFor="wf-context-step-name">Step name</FieldLabel>
            <Input
              autoCapitalize="off"
              disabled={disabled}
              id="wf-context-step-name"
              onChange={(event) =>
                updateStep(selectedStep.index, {
                  ...selectedStep.step,
                  name: event.target.value,
                })
              }
              placeholder="Human-friendly label"
              value={selectedStep.step.name ?? ""}
            />
          </OptionalFieldToggle>

          <div className="space-y-1.5">
            <FieldLabel htmlFor="wf-context-step-action">Action</FieldLabel>
            <WorkflowStepActionSelect
              disabled={disabled}
              id="wf-context-step-action"
              onChange={(step) => updateStep(selectedStep.index, step)}
              step={selectedStep.step}
            />
          </div>

          <OptionalFieldToggle
            checked={selectedStep.step.timeoutSecs !== undefined}
            disabled={disabled}
            id="wf-context-step-timeout-toggle"
            label="Timeout"
            onCheckedChange={(checked) =>
              updateStep(selectedStep.index, {
                ...selectedStep.step,
                timeoutSecs: checked
                  ? (selectedStep.step.timeoutSecs ?? "")
                  : undefined,
              })
            }
          >
            <FieldLabel htmlFor="wf-context-step-timeout-secs">
              Timeout seconds
            </FieldLabel>
            <Input
              autoCapitalize="off"
              disabled={disabled}
              id="wf-context-step-timeout-secs"
              inputMode="numeric"
              onChange={(event) =>
                updateStep(selectedStep.index, {
                  ...selectedStep.step,
                  timeoutSecs: event.target.value,
                })
              }
              placeholder="e.g. 300"
              value={selectedStep.step.timeoutSecs ?? ""}
            />
          </OptionalFieldToggle>

          <OptionalFieldToggle
            checked={selectedStep.step.condition !== undefined}
            disabled={disabled}
            id="wf-context-step-condition-toggle"
            label="Condition"
            onCheckedChange={(checked) =>
              updateStep(selectedStep.index, {
                ...selectedStep.step,
                condition: checked
                  ? (selectedStep.step.condition ?? "")
                  : undefined,
              })
            }
          >
            <FieldLabel htmlFor="wf-context-step-condition">
              Run condition
            </FieldLabel>
            <Input
              autoCapitalize="off"
              disabled={disabled}
              id="wf-context-step-condition"
              onChange={(event) =>
                updateStep(selectedStep.index, {
                  ...selectedStep.step,
                  condition: event.target.value,
                })
              }
              placeholder='e.g. str_contains(trigger_text, "deploy")'
              value={selectedStep.step.condition ?? ""}
            />
          </OptionalFieldToggle>

          <WorkflowStepActionFields
            disabled={disabled}
            onUpdate={(step) => updateStep(selectedStep.index, step)}
            prefix="wf-context-step"
            step={selectedStep.step}
            triggerType={formState.trigger.on as TriggerType}
            useOptionalToggles
          />
        </>
      ) : isTriggerSelected ? (
        <>
          <h3 className="text-sm font-semibold">Trigger</h3>

          <div className="space-y-1.5">
            <FieldLabel htmlFor="wf-context-trigger-type">Trigger</FieldLabel>
            <FormSelect
              disabled={disabled}
              id="wf-context-trigger-type"
              onChange={(value) =>
                updateWorkflow({
                  trigger: { on: value as TriggerType },
                })
              }
              value={formState.trigger.on}
            >
              {TRIGGER_TYPES.map((type) => (
                <option key={type} value={type}>
                  {TRIGGER_LABELS[type]}
                </option>
              ))}
            </FormSelect>
          </div>

          <TriggerConfigFields
            disabled={disabled}
            onUpdate={(trigger) => updateWorkflow({ trigger })}
            trigger={formState.trigger}
            useOptionalToggles
          />
        </>
      ) : (
        <>
          <h3 className="text-sm font-semibold">Workflow</h3>

          <div className="space-y-1.5">
            <FieldLabel htmlFor="wf-context-name">Workflow title</FieldLabel>
            <Input
              autoCapitalize="off"
              autoCorrect="off"
              disabled={disabled}
              id="wf-context-name"
              onChange={(event) => updateWorkflow({ name: event.target.value })}
              placeholder="e.g. Deploy notifications"
              value={formState.name}
            />
          </div>

          <OptionalFieldToggle
            checked={showDescriptionField}
            disabled={disabled}
            id="wf-context-description-toggle"
            label="Description"
            onCheckedChange={(checked) => {
              setShowDescriptionField(checked);
              if (!checked) {
                updateWorkflow({ description: "" });
              }
            }}
          >
            <FieldLabel htmlFor="wf-context-description">Description</FieldLabel>
            <Textarea
              autoCapitalize="off"
              className="min-h-[72px] resize-y text-sm"
              disabled={disabled}
              id="wf-context-description"
              onChange={(event) =>
                updateWorkflow({ description: event.target.value })
              }
              placeholder="What does this workflow do?"
              value={formState.description}
            />
          </OptionalFieldToggle>

          <div className="flex items-center gap-2">
            <Checkbox
              checked={formState.enabled}
              disabled={disabled}
              id="wf-context-enabled"
              onCheckedChange={(checked) =>
                updateWorkflow({ enabled: checked === true })
              }
            />
            <label className="text-sm" htmlFor="wf-context-enabled">
              Workflow is enabled
            </label>
          </div>

          {channelName ? (
            <div className="space-y-1">
              <FieldLabel>Channel</FieldLabel>
              <p className="text-sm text-muted-foreground">{channelName}</p>
            </div>
          ) : null}
        </>
      )}

      {errorMessage ? (
        <p className="rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {errorMessage}
        </p>
      ) : null}

      <Button
        className="w-full gap-2"
        disabled={disabled}
        onClick={onSave}
        type="button"
      >
        <Save className="h-4 w-4" />
        {isSaving ? "Saving..." : "Save changes"}
      </Button>
    </div>
  );
}
