import type { ReactNode } from "react";

import type { Workflow } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import {
  getWorkflowDescription,
  getWorkflowDisplayStatus,
  getWorkflowDisplayTitle,
  getWorkflowEnabled,
  getWorkflowTriggerSummary,
} from "./workflowDefinition";
import {
  getStepDetails,
  getStepTitle,
  getTriggerDetails,
  getTriggerTitle,
  resolveGraphNodeSelection,
} from "./workflowDefinitionNodeInfo";

type WorkflowPropertiesPanelProps = {
  workflow: Workflow;
  channelName?: string | null;
  selectedNodeId: string | null;
  className?: string;
};

function PropertyBlock({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="space-y-1">
      <p className="text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
        {label}
      </p>
      <div className="text-sm text-foreground">{children}</div>
    </div>
  );
}

function formatStatusLabel(status: string) {
  return status.replace(/_/g, " ");
}

function StatusBadge({ status }: { status: string }) {
  const colors: Record<string, string> = {
    active: "bg-green-500/15 text-green-500",
    disabled: "bg-muted text-muted-foreground",
    archived: "bg-amber-500/15 text-amber-500",
    completed: "bg-green-500/15 text-green-500",
    failed: "bg-red-500/15 text-red-500",
    running: "bg-blue-500/15 text-blue-500",
    pending: "bg-muted text-muted-foreground",
    cancelled: "bg-muted text-muted-foreground",
    waiting_approval: "bg-amber-500/15 text-amber-500",
  };

  return (
    <span
      className={`inline-flex rounded-full px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.16em] ${colors[status] ?? colors.pending}`}
    >
      {formatStatusLabel(status)}
    </span>
  );
}

export function WorkflowPropertiesPanel({
  workflow,
  channelName,
  selectedNodeId,
  className,
}: WorkflowPropertiesPanelProps) {
  const def = workflow.definition;
  const resolved = resolveGraphNodeSelection(def, selectedNodeId);

  if (resolved.kind === "trigger") {
    const title = getTriggerTitle(resolved.trigger);
    const lines = getTriggerDetails(resolved.trigger);
    return (
      <div
        className={cn("flex flex-col gap-4 p-4", className)}
        data-testid="workflow-properties-panel"
      >
        <div>
          <h3 className="text-sm font-semibold">Trigger</h3>
          <p className="mt-1 text-xs text-muted-foreground">
            Read-only summary. Use Edit to change the workflow.
          </p>
        </div>
        <PropertyBlock label="Type">{title}</PropertyBlock>
        {lines.length > 0 ? (
          <div className="space-y-2">
            <p className="text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
              Details
            </p>
            <ul className="list-inside list-disc space-y-1 text-sm text-muted-foreground">
              {lines.map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
          </div>
        ) : null}
      </div>
    );
  }

  if (resolved.kind === "step") {
    const { step, index, stepId } = resolved;
    const title = getStepTitle(step, index);
    const lines = getStepDetails(step);
    return (
      <div
        className={cn("flex flex-col gap-4 p-4", className)}
        data-testid="workflow-properties-panel"
      >
        <div>
          <h3 className="text-sm font-semibold">
            Step {index + 1}: {title}
          </h3>
          <p className="mt-1 font-mono text-xs text-muted-foreground">
            {stepId}
          </p>
          <p className="mt-2 text-xs text-muted-foreground">
            Read-only summary. Use Edit to change the workflow.
          </p>
        </div>
        {lines.length > 0 ? (
          <div className="space-y-2">
            <p className="text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
              Details
            </p>
            <ul className="list-inside list-disc space-y-1 text-sm text-muted-foreground">
              {lines.map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">
            No extra fields for this step.
          </p>
        )}
      </div>
    );
  }

  const description = getWorkflowDescription(def);
  const triggerSummary = getWorkflowTriggerSummary(def);
  const workflowStatus = getWorkflowDisplayStatus(workflow);
  const enabled = getWorkflowEnabled(def);

  return (
    <div
      className={cn("flex flex-col gap-4 p-4", className)}
      data-testid="workflow-properties-panel"
    >
      <div>
        <h3 className="text-sm font-semibold">Workflow</h3>
        <p className="mt-1 text-xs text-muted-foreground">
          Description and trigger summary are below. Select a node on the
          diagram to focus the trigger or a step. Use Edit to make changes.
        </p>
      </div>

      <PropertyBlock label="Title">
        {getWorkflowDisplayTitle(workflow)}
      </PropertyBlock>

      {description ? (
        <PropertyBlock label="Description">{description}</PropertyBlock>
      ) : null}

      {channelName ? (
        <PropertyBlock label="Channel">{channelName}</PropertyBlock>
      ) : null}

      <PropertyBlock label="Status">
        <span className="inline-flex items-center gap-2">
          <StatusBadge status={workflowStatus} />
          <span className="text-muted-foreground">
            {enabled ? "Enabled in definition" : "Disabled in definition"}
          </span>
        </span>
      </PropertyBlock>

      {triggerSummary ? (
        <PropertyBlock label="Trigger">{triggerSummary}</PropertyBlock>
      ) : null}

      <details className="rounded-lg border border-border/60 bg-muted/20">
        <summary className="cursor-pointer select-none px-3 py-2 text-xs font-medium text-muted-foreground">
          Raw definition (JSON)
        </summary>
        <pre className="max-h-48 overflow-auto border-t border-border/60 p-3 font-mono text-[11px] leading-relaxed">
          {JSON.stringify(def, null, 2)}
        </pre>
      </details>
    </div>
  );
}
