import { Code, Plus } from "lucide-react";
import * as React from "react";

import { Button } from "@/shared/ui/button";
import { Checkbox } from "@/shared/ui/checkbox";
import { Input } from "@/shared/ui/input";
import { Textarea } from "@/shared/ui/textarea";
import { TriggerConfigFields } from "./WorkflowEditorFields";
import { WorkflowStepCard } from "./WorkflowStepCard";
import { FieldLabel, FormSelect } from "./workflowFormPrimitives";
import {
  DEFAULT_FORM_STATE,
  TRIGGER_LABELS,
  TRIGGER_TYPES,
  formStateToYaml,
  getWorkflowFormValidationError,
  getWorkflowYamlValidationError,
  nextStepId,
  yamlToFormState,
} from "./workflowFormTypes";
import type {
  StepFormState,
  TriggerType,
  WorkflowFormState,
} from "./workflowFormTypes";

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

type WorkflowFormBuilderProps = {
  disabled?: boolean;
  onChange: (yaml: string) => void;
  yaml: string;
};

export function WorkflowFormBuilder({
  disabled,
  onChange,
  yaml,
}: WorkflowFormBuilderProps) {
  // Parse once on mount instead of calling yamlToFormState three times
  const initialParseRef = React.useRef(yaml ? yamlToFormState(yaml) : null);
  const [mode, setMode] = React.useState<"form" | "yaml">(
    initialParseRef.current === null || initialParseRef.current.ok
      ? "form"
      : "yaml",
  );
  const [formState, setFormState] = React.useState<WorkflowFormState>(
    initialParseRef.current?.ok
      ? initialParseRef.current.state
      : DEFAULT_FORM_STATE,
  );
  const [parseError, setParseError] = React.useState<string | null>(
    initialParseRef.current !== null && !initialParseRef.current.ok
      ? initialParseRef.current.error
      : null,
  );

  const updateFormState = React.useCallback(
    (next: WorkflowFormState) => {
      setFormState(next);
      onChange(formStateToYaml(next));
    },
    [onChange],
  );

  const handleToggleMode = React.useCallback(() => {
    if (mode === "form") {
      setMode("yaml");
      setParseError(null);
    } else {
      const result = yamlToFormState(yaml);
      if (result.ok) {
        setFormState(result.state);
        setParseError(null);
        setMode("form");
      } else {
        setParseError(result.error);
      }
    }
  }, [mode, yaml]);

  const addStep = React.useCallback(() => {
    updateFormState({
      ...formState,
      steps: [
        ...formState.steps,
        { id: nextStepId(formState.steps), action: "delay" },
      ],
    });
  }, [formState, updateFormState]);

  const removeStep = React.useCallback(
    (index: number) => {
      updateFormState({
        ...formState,
        steps: formState.steps.filter((_, i) => i !== index),
      });
    },
    [formState, updateFormState],
  );

  const updateStep = React.useCallback(
    (index: number, step: StepFormState) => {
      const next = [...formState.steps];
      next[index] = step;
      updateFormState({ ...formState, steps: next });
    },
    [formState, updateFormState],
  );

  const validationError = React.useMemo(
    () =>
      mode === "yaml"
        ? getWorkflowYamlValidationError(yaml)
        : getWorkflowFormValidationError(formState),
    [formState, mode, yaml],
  );

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-end">
        <Button
          className="h-7 gap-1.5 text-xs"
          disabled={disabled}
          onClick={handleToggleMode}
          size="sm"
          type="button"
          variant="ghost"
        >
          <Code className="h-3.5 w-3.5" />
          {mode === "form" ? "Edit as YAML" : "Back to form"}
        </Button>
      </div>

      {parseError ? (
        <p className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          Cannot switch to form view: {parseError}
        </p>
      ) : null}

      {validationError ? (
        <p className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {validationError}
        </p>
      ) : null}

      {mode === "yaml" ? (
        <div className="space-y-1.5">
          <Textarea
            autoCapitalize="off"
            className="min-h-[240px] resize-y font-mono text-xs"
            disabled={disabled}
            onChange={(event) => onChange(event.target.value)}
            value={yaml}
          />
          <p className="text-xs text-muted-foreground">
            Edit the raw YAML definition directly.
          </p>
        </div>
      ) : (
        <div className="space-y-4">
          <div className="space-y-1.5">
            <FieldLabel htmlFor="wf-name">Workflow title</FieldLabel>
            <Input
              autoCapitalize="off"
              autoCorrect="off"
              disabled={disabled}
              id="wf-name"
              onChange={(event) =>
                updateFormState({ ...formState, name: event.target.value })
              }
              placeholder="e.g. Deploy notifications"
              value={formState.name}
            />
          </div>

          <div className="space-y-1.5">
            <FieldLabel htmlFor="wf-description">
              Description (optional)
            </FieldLabel>
            <Textarea
              autoCapitalize="off"
              className="min-h-[72px] resize-y text-sm"
              disabled={disabled}
              id="wf-description"
              onChange={(event) =>
                updateFormState({
                  ...formState,
                  description: event.target.value,
                })
              }
              placeholder="What does this workflow do?"
              value={formState.description}
            />
          </div>

          <div className="flex items-center gap-2 rounded-md border border-border/70 px-3 py-2">
            <Checkbox
              checked={formState.enabled}
              disabled={disabled}
              id="wf-enabled"
              onCheckedChange={(checked) =>
                updateFormState({
                  ...formState,
                  enabled: checked === true,
                })
              }
            />
            <label className="text-sm" htmlFor="wf-enabled">
              Workflow is enabled
            </label>
          </div>

          <div className="space-y-3">
            <div className="space-y-1.5">
              <FieldLabel htmlFor="wf-trigger-type">Trigger</FieldLabel>
              <FormSelect
                disabled={disabled}
                id="wf-trigger-type"
                onChange={(value) =>
                  updateFormState({
                    ...formState,
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
              onUpdate={(trigger) => updateFormState({ ...formState, trigger })}
              trigger={formState.trigger}
            />
          </div>

          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <FieldLabel>Steps</FieldLabel>
              <Button
                className="h-7 gap-1.5 text-xs"
                disabled={disabled}
                onClick={addStep}
                size="sm"
                type="button"
                variant="outline"
              >
                <Plus className="h-3.5 w-3.5" />
                Add step
              </Button>
            </div>

            {formState.steps.length === 0 ? (
              <p className="py-4 text-center text-xs text-muted-foreground">
                No steps yet — add one to get started.
              </p>
            ) : (
              <div className="space-y-2">
                {formState.steps.map((step, index) => (
                  <WorkflowStepCard
                    disabled={disabled}
                    index={index}
                    key={step.id}
                    onRemove={() => removeStep(index)}
                    onUpdate={(updated) => updateStep(index, updated)}
                    step={step}
                    triggerType={formState.trigger.on}
                  />
                ))}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
