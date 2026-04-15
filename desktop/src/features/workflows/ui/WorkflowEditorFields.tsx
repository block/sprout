import { Checkbox } from "@/shared/ui/checkbox";
import { Input } from "@/shared/ui/input";
import { cn } from "@/shared/lib/cn";
import { Textarea } from "@/shared/ui/textarea";
import { WorkflowWebhookHeadersEditor } from "./WorkflowWebhookHeadersEditor";
import { FieldLabel, FormSelect } from "./workflowFormPrimitives";
import { ACTION_LABELS, ACTION_TYPES } from "./workflowFormTypes";
import type {
  ActionType,
  HeaderFormState,
  StepFormState,
  TriggerConfig,
  TriggerType,
} from "./workflowFormTypes";

export function OptionalFieldToggle({
  checked,
  children,
  className,
  disabled,
  id,
  label,
  onCheckedChange,
}: {
  checked: boolean;
  children: React.ReactNode;
  className?: string;
  disabled?: boolean;
  id: string;
  label: string;
  onCheckedChange: (checked: boolean) => void;
}) {
  return (
    <div className={cn("space-y-2", className)}>
      <div className="flex items-center gap-2">
        <Checkbox
          checked={checked}
          disabled={disabled}
          id={id}
          onCheckedChange={(next) => onCheckedChange(next === true)}
        />
        <label className="block text-sm text-foreground" htmlFor={id}>
          {label}
        </label>
      </div>
      {checked ? <div className="space-y-1.5 pl-5">{children}</div> : null}
    </div>
  );
}

export function TriggerConfigFields({
  disabled,
  trigger,
  onUpdate,
  useOptionalToggles = false,
}: {
  disabled?: boolean;
  trigger: TriggerConfig;
  onUpdate: (trigger: TriggerConfig) => void;
  useOptionalToggles?: boolean;
}) {
  switch (trigger.on) {
    case "message_posted":
    case "diff_posted":
      if (useOptionalToggles) {
        const hasFilter = trigger.filter !== undefined;
        return (
          <OptionalFieldToggle
            checked={hasFilter}
            disabled={disabled}
            id="wf-trigger-filter-toggle"
            label="Filter"
            onCheckedChange={(checked) =>
              onUpdate({
                ...trigger,
                filter: checked ? (trigger.filter ?? "") : undefined,
              })
            }
          >
            <FieldLabel htmlFor="wf-trigger-filter">Filter expression</FieldLabel>
            <Input
              autoCapitalize="off"
              disabled={disabled}
              id="wf-trigger-filter"
              onChange={(event) =>
                onUpdate({ ...trigger, filter: event.target.value })
              }
              placeholder='e.g. contains(text, "deploy")'
              value={trigger.filter ?? ""}
            />
          </OptionalFieldToggle>
        );
      }
      return (
        <div className="space-y-1.5">
          <FieldLabel htmlFor="wf-trigger-filter">
            Filter expression (optional)
          </FieldLabel>
          <Input
            autoCapitalize="off"
            disabled={disabled}
            id="wf-trigger-filter"
            onChange={(event) =>
              onUpdate({ ...trigger, filter: event.target.value })
            }
            placeholder='e.g. contains(text, "deploy")'
            value={trigger.filter ?? ""}
          />
          <p className="text-xs text-muted-foreground">
            Evalexpr filter — leave empty to trigger on all matching events.
          </p>
        </div>
      );
    case "reaction_added":
      if (useOptionalToggles) {
        const hasEmoji = trigger.emoji !== undefined;
        return (
          <OptionalFieldToggle
            checked={hasEmoji}
            disabled={disabled}
            id="wf-trigger-emoji-toggle"
            label="Emoji"
            onCheckedChange={(checked) =>
              onUpdate({
                ...trigger,
                emoji: checked ? (trigger.emoji ?? "") : undefined,
              })
            }
          >
            <FieldLabel htmlFor="wf-trigger-emoji">Emoji</FieldLabel>
            <Input
              autoCapitalize="off"
              disabled={disabled}
              id="wf-trigger-emoji"
              onChange={(event) =>
                onUpdate({ ...trigger, emoji: event.target.value })
              }
              placeholder="e.g. thumbsup"
              value={trigger.emoji ?? ""}
            />
          </OptionalFieldToggle>
        );
      }
      return (
        <div className="space-y-1.5">
          <FieldLabel htmlFor="wf-trigger-emoji">
            Emoji filter (optional)
          </FieldLabel>
          <Input
            autoCapitalize="off"
            disabled={disabled}
            id="wf-trigger-emoji"
            onChange={(event) =>
              onUpdate({ ...trigger, emoji: event.target.value })
            }
            placeholder="e.g. thumbsup"
            value={trigger.emoji ?? ""}
          />
          <p className="text-xs text-muted-foreground">
            Leave empty to trigger on any reaction.
          </p>
        </div>
      );
    case "webhook":
      return (
        <p className="text-xs text-muted-foreground">
          Webhook URL is generated on create.
        </p>
      );
    case "schedule":
      if (useOptionalToggles) {
        const hasCron = trigger.cron !== undefined;
        const hasInterval = trigger.interval !== undefined;
        return (
          <div className="space-y-3">
            <OptionalFieldToggle
              checked={hasCron}
              disabled={disabled}
              id="wf-trigger-cron-toggle"
              label="Cron"
              onCheckedChange={(checked) =>
                onUpdate({
                  ...trigger,
                  cron: checked ? (trigger.cron ?? "") : undefined,
                  interval: checked ? undefined : trigger.interval,
                })
              }
            >
              <FieldLabel htmlFor="wf-trigger-cron">Cron expression</FieldLabel>
              <Input
                autoCapitalize="off"
                disabled={disabled}
                id="wf-trigger-cron"
                onChange={(event) =>
                  onUpdate({ ...trigger, cron: event.target.value })
                }
                placeholder="e.g. 0 9 * * 1-5"
                value={trigger.cron ?? ""}
              />
            </OptionalFieldToggle>

            <OptionalFieldToggle
              checked={hasInterval}
              disabled={disabled}
              id="wf-trigger-interval-toggle"
              label="Interval"
              onCheckedChange={(checked) =>
                onUpdate({
                  ...trigger,
                  interval: checked ? (trigger.interval ?? "") : undefined,
                  cron: checked ? undefined : trigger.cron,
                })
              }
            >
              <FieldLabel htmlFor="wf-trigger-interval">Interval</FieldLabel>
              <Input
                autoCapitalize="off"
                disabled={disabled}
                id="wf-trigger-interval"
                onChange={(event) =>
                  onUpdate({ ...trigger, interval: event.target.value })
                }
                placeholder="e.g. 1h, 30m"
                value={trigger.interval ?? ""}
              />
            </OptionalFieldToggle>
          </div>
        );
      }
      return (
        <div className="space-y-3">
          <div className="space-y-1.5">
            <FieldLabel htmlFor="wf-trigger-cron">
              Cron expression (optional)
            </FieldLabel>
            <Input
              autoCapitalize="off"
              disabled={disabled}
              id="wf-trigger-cron"
              onChange={(event) =>
                onUpdate({ ...trigger, cron: event.target.value })
              }
              placeholder="e.g. 0 9 * * 1-5 (weekdays at 9am UTC)"
              value={trigger.cron ?? ""}
            />
          </div>
          <div className="space-y-1.5">
            <FieldLabel htmlFor="wf-trigger-interval">
              Interval (optional)
            </FieldLabel>
            <Input
              autoCapitalize="off"
              disabled={disabled}
              id="wf-trigger-interval"
              onChange={(event) =>
                onUpdate({ ...trigger, interval: event.target.value })
              }
              placeholder="e.g. 1h, 30m"
              value={trigger.interval ?? ""}
            />
          </div>
          <p className="text-xs text-muted-foreground">
            Provide either a cron expression or a simple interval.
          </p>
        </div>
      );
    default:
      return null;
  }
}

function BackendSupportHint({ action }: { action: StepFormState["action"] }) {
  switch (action) {
    case "send_dm":
      return (
        <p className="text-xs text-amber-700">
          `send_dm` is not executed yet.
        </p>
      );
    case "set_channel_topic":
      return (
        <p className="text-xs text-amber-700">
          `set_channel_topic` is not executed yet.
        </p>
      );
    case "request_approval":
      return (
        <p className="text-xs text-amber-700">
          Approval runs are not fully supported yet.
        </p>
      );
    default:
      return null;
  }
}

export function WorkflowStepActionFields({
  disabled,
  onUpdate,
  prefix,
  step,
  triggerType,
  useOptionalToggles = false,
}: {
  disabled?: boolean;
  onUpdate: (step: StepFormState) => void;
  prefix: string;
  step: StepFormState;
  triggerType: TriggerType;
  useOptionalToggles?: boolean;
}) {
  const createFirstHeader = (): HeaderFormState[] => [
    {
      id: `${step.id || prefix}_header_1`,
      key: "",
      value: "",
    },
  ];

  switch (step.action) {
    case "delay":
      return (
        <div className="space-y-1.5">
          <FieldLabel htmlFor={`${prefix}-duration`}>Duration</FieldLabel>
          <Input
            autoCapitalize="off"
            disabled={disabled}
            id={`${prefix}-duration`}
            onChange={(event) =>
              onUpdate({ ...step, duration: event.target.value })
            }
            placeholder="e.g. 5s, 1m, 1h"
            value={step.duration ?? ""}
          />
        </div>
      );
    case "send_message":
      if (useOptionalToggles) {
        const hasChannelOverride = step.channel !== undefined;
        return (
          <div className="space-y-3">
            <div className="space-y-1.5">
              <FieldLabel htmlFor={`${prefix}-text`}>Message text</FieldLabel>
              <Textarea
                autoCapitalize="off"
                className="min-h-[60px] resize-y text-xs"
                disabled={disabled}
                id={`${prefix}-text`}
                onChange={(event) =>
                  onUpdate({ ...step, text: event.target.value })
                }
                placeholder="e.g. Deployment started by {{trigger.author}}"
                value={step.text ?? ""}
              />
            </div>
            <OptionalFieldToggle
              checked={hasChannelOverride}
              disabled={disabled}
              id={`${prefix}-channel-toggle`}
              label="Override channel"
              onCheckedChange={(checked) =>
                onUpdate({
                  ...step,
                  channel: checked ? (step.channel ?? "") : undefined,
                })
              }
            >
              <FieldLabel htmlFor={`${prefix}-channel`}>Channel ID</FieldLabel>
              <Input
                autoCapitalize="off"
                disabled={disabled}
                id={`${prefix}-channel`}
                onChange={(event) =>
                  onUpdate({ ...step, channel: event.target.value })
                }
                placeholder="Channel UUID"
                value={step.channel ?? ""}
              />
              {triggerType === "webhook" && !(step.channel ?? "").trim() ? (
                <p className="text-xs text-amber-700">
                  Required for webhook trigger.
                </p>
              ) : null}
            </OptionalFieldToggle>
          </div>
        );
      }
      return (
        <div className="space-y-2">
          <div className="space-y-1.5">
            <FieldLabel htmlFor={`${prefix}-text`}>Message text</FieldLabel>
            <Textarea
              autoCapitalize="off"
              className="min-h-[60px] resize-y text-xs"
              disabled={disabled}
              id={`${prefix}-text`}
              onChange={(event) =>
                onUpdate({ ...step, text: event.target.value })
              }
              placeholder="e.g. Deployment started by {{trigger.author}}"
              value={step.text ?? ""}
            />
          </div>
          <div className="space-y-1.5">
            <FieldLabel htmlFor={`${prefix}-channel`}>
              Channel override (optional)
            </FieldLabel>
            <Input
              autoCapitalize="off"
              disabled={disabled}
              id={`${prefix}-channel`}
              onChange={(event) =>
                onUpdate({ ...step, channel: event.target.value })
              }
              placeholder="Channel UUID"
              value={step.channel ?? ""}
            />
            <p className="text-xs text-muted-foreground">
              Leave empty to use the trigger channel. Webhook runs and manual
              Trigger runs need an explicit channel override.
            </p>
            {triggerType === "webhook" && !(step.channel ?? "").trim() ? (
              <p className="text-xs text-amber-700">
                This step will fail for webhook-triggered runs until a channel
                override is set.
              </p>
            ) : null}
          </div>
        </div>
      );
    case "send_dm":
      return (
        <div className="space-y-2">
          <BackendSupportHint action={step.action} />
          <div className="space-y-1.5">
            <FieldLabel htmlFor={`${prefix}-to`}>To (pubkey)</FieldLabel>
            <Input
              autoCapitalize="off"
              disabled={disabled}
              id={`${prefix}-to`}
              onChange={(event) =>
                onUpdate({ ...step, to: event.target.value })
              }
              placeholder="e.g. {{trigger.author}} or hex pubkey"
              value={step.to ?? ""}
            />
          </div>
          <div className="space-y-1.5">
            <FieldLabel htmlFor={`${prefix}-text`}>Message text</FieldLabel>
            <Textarea
              autoCapitalize="off"
              className="min-h-[60px] resize-y text-xs"
              disabled={disabled}
              id={`${prefix}-text`}
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
      if (useOptionalToggles) {
        const hasMethodOverride = step.method !== undefined;
        const hasHeaders = step.headers !== undefined;
        const hasBody = step.body !== undefined;
        return (
          <div className="space-y-3">
            <div className="space-y-1.5">
              <FieldLabel htmlFor={`${prefix}-url`}>URL</FieldLabel>
              <Input
                autoCapitalize="off"
                disabled={disabled}
                id={`${prefix}-url`}
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

            <OptionalFieldToggle
              checked={hasMethodOverride}
              disabled={disabled}
              id={`${prefix}-method-toggle`}
              label="Method"
              onCheckedChange={(checked) =>
                onUpdate({
                  ...step,
                  method: checked ? (step.method ?? "POST") : undefined,
                })
              }
            >
              <FieldLabel htmlFor={`${prefix}-method`}>Method</FieldLabel>
              <FormSelect
                disabled={disabled}
                id={`${prefix}-method`}
                onChange={(value) => onUpdate({ ...step, method: value })}
                value={step.method ?? "POST"}
              >
                <option value="POST">POST</option>
                <option value="GET">GET</option>
                <option value="PUT">PUT</option>
                <option value="PATCH">PATCH</option>
                <option value="DELETE">DELETE</option>
              </FormSelect>
            </OptionalFieldToggle>

            <OptionalFieldToggle
              checked={hasHeaders}
              disabled={disabled}
              id={`${prefix}-headers-toggle`}
              label="Headers"
              onCheckedChange={(checked) =>
                onUpdate({
                  ...step,
                  headers: checked ? (step.headers ?? createFirstHeader()) : undefined,
                })
              }
            >
              <WorkflowWebhookHeadersEditor
                disabled={disabled}
                headers={step.headers ?? []}
                hideLabel
                onChange={(headers) => onUpdate({ ...step, headers })}
                stepId={step.id || prefix}
              />
            </OptionalFieldToggle>

            <OptionalFieldToggle
              checked={hasBody}
              disabled={disabled}
              id={`${prefix}-body-toggle`}
              label="Body"
              onCheckedChange={(checked) =>
                onUpdate({
                  ...step,
                  body: checked ? (step.body ?? "") : undefined,
                })
              }
            >
              <FieldLabel htmlFor={`${prefix}-body`}>Body</FieldLabel>
              <Textarea
                autoCapitalize="off"
                className="min-h-[60px] resize-y font-mono text-xs"
                disabled={disabled}
                id={`${prefix}-body`}
                onChange={(event) =>
                  onUpdate({ ...step, body: event.target.value })
                }
                placeholder='{"key": "{{trigger.text}}"}'
                value={step.body ?? ""}
              />
            </OptionalFieldToggle>
          </div>
        );
      }
      return (
        <div className="space-y-3">
          <div className="space-y-1.5">
            <FieldLabel htmlFor={`${prefix}-url`}>URL</FieldLabel>
            <Input
              autoCapitalize="off"
              disabled={disabled}
              id={`${prefix}-url`}
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
            <FieldLabel htmlFor={`${prefix}-method`}>
              Method (optional)
            </FieldLabel>
            <FormSelect
              disabled={disabled}
              id={`${prefix}-method`}
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
          <WorkflowWebhookHeadersEditor
            disabled={disabled}
            headers={step.headers ?? []}
            onChange={(headers) => onUpdate({ ...step, headers })}
            stepId={step.id || prefix}
          />
          <div className="space-y-1.5">
            <FieldLabel htmlFor={`${prefix}-body`}>Body (optional)</FieldLabel>
            <Textarea
              autoCapitalize="off"
              className="min-h-[60px] resize-y font-mono text-xs"
              disabled={disabled}
              id={`${prefix}-body`}
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
      if (useOptionalToggles) {
        const hasTimeout = step.timeout !== undefined;
        return (
          <div className="space-y-3">
            <BackendSupportHint action={step.action} />
            <div className="space-y-1.5">
              <FieldLabel htmlFor={`${prefix}-from`}>From (approver)</FieldLabel>
              <Input
                autoCapitalize="off"
                disabled={disabled}
                id={`${prefix}-from`}
                onChange={(event) =>
                  onUpdate({ ...step, from: event.target.value })
                }
                placeholder="Pubkey or role"
                value={step.from ?? ""}
              />
            </div>
            <div className="space-y-1.5">
              <FieldLabel htmlFor={`${prefix}-message`}>Message</FieldLabel>
              <Input
                autoCapitalize="off"
                disabled={disabled}
                id={`${prefix}-message`}
                onChange={(event) =>
                  onUpdate({ ...step, message: event.target.value })
                }
                placeholder="Approval request message"
                value={step.message ?? ""}
              />
            </div>
            <OptionalFieldToggle
              checked={hasTimeout}
              disabled={disabled}
              id={`${prefix}-timeout-toggle`}
              label="Timeout"
              onCheckedChange={(checked) =>
                onUpdate({
                  ...step,
                  timeout: checked ? (step.timeout ?? "") : undefined,
                })
              }
            >
              <FieldLabel htmlFor={`${prefix}-timeout`}>Timeout</FieldLabel>
              <Input
                autoCapitalize="off"
                disabled={disabled}
                id={`${prefix}-timeout`}
                onChange={(event) =>
                  onUpdate({ ...step, timeout: event.target.value })
                }
                placeholder="e.g. 24h"
                value={step.timeout ?? ""}
              />
            </OptionalFieldToggle>
          </div>
        );
      }
      return (
        <div className="space-y-2">
          <BackendSupportHint action={step.action} />
          <div className="space-y-1.5">
            <FieldLabel htmlFor={`${prefix}-from`}>From (approver)</FieldLabel>
            <Input
              autoCapitalize="off"
              disabled={disabled}
              id={`${prefix}-from`}
              onChange={(event) =>
                onUpdate({ ...step, from: event.target.value })
              }
              placeholder="Pubkey or role"
              value={step.from ?? ""}
            />
          </div>
          <div className="space-y-1.5">
            <FieldLabel htmlFor={`${prefix}-message`}>Message</FieldLabel>
            <Input
              autoCapitalize="off"
              disabled={disabled}
              id={`${prefix}-message`}
              onChange={(event) =>
                onUpdate({ ...step, message: event.target.value })
              }
              placeholder="Approval request message"
              value={step.message ?? ""}
            />
          </div>
          <div className="space-y-1.5">
            <FieldLabel htmlFor={`${prefix}-timeout`}>
              Timeout (optional)
            </FieldLabel>
            <Input
              autoCapitalize="off"
              disabled={disabled}
              id={`${prefix}-timeout`}
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
          <FieldLabel htmlFor={`${prefix}-emoji`}>Emoji</FieldLabel>
          <Input
            autoCapitalize="off"
            disabled={disabled}
            id={`${prefix}-emoji`}
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
        <div className="space-y-2">
          <BackendSupportHint action={step.action} />
          <div className="space-y-1.5">
            <FieldLabel htmlFor={`${prefix}-topic`}>Topic</FieldLabel>
            <Input
              autoCapitalize="off"
              disabled={disabled}
              id={`${prefix}-topic`}
              onChange={(event) =>
                onUpdate({ ...step, topic: event.target.value })
              }
              placeholder="New channel topic"
              value={step.topic ?? ""}
            />
          </div>
        </div>
      );
    default:
      return null;
  }
}

export function WorkflowStepActionSelect({
  disabled,
  onChange,
  step,
  id = "wf-context-step-action",
}: {
  disabled?: boolean;
  onChange: (step: StepFormState) => void;
  step: StepFormState;
  id?: string;
}) {
  return (
    <FormSelect
      disabled={disabled}
      id={id}
      onChange={(value) => {
        const next = { ...step, action: value as ActionType };
        if (value === "call_webhook" && !next.method) {
          next.method = "POST";
        }
        onChange(next);
      }}
      value={step.action}
    >
      {ACTION_TYPES.map((action) => (
        <option key={action} value={action}>
          {ACTION_LABELS[action]}
        </option>
      ))}
    </FormSelect>
  );
}
