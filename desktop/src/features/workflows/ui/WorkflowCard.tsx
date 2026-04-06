import {
  Clock,
  Copy,
  MoreHorizontal,
  Pencil,
  Play,
  Trash2,
  Zap,
} from "lucide-react";

import type { Workflow } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import {
  getWorkflowDescription,
  getWorkflowDisplayStatus,
  getWorkflowTriggerSummary,
} from "./workflowDefinition";

type WorkflowCardProps = {
  workflow: Workflow;
  channelName?: string;
  onSelect: (workflowId: string) => void;
  onTrigger: (workflowId: string) => void;
  onEdit: (workflow: Workflow) => void;
  onDuplicate: (workflow: Workflow) => void;
  onDelete: (workflow: Workflow) => void;
};

function StatusBadge({ status }: { status: Workflow["status"] }) {
  const colors = {
    active: "bg-green-500/15 text-green-500",
    disabled: "bg-muted text-muted-foreground",
    archived: "bg-amber-500/15 text-amber-500",
  };

  return (
    <span
      className={`inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium ${colors[status]}`}
    >
      {status}
    </span>
  );
}

export function WorkflowCard({
  workflow,
  channelName,
  onSelect,
  onTrigger,
  onEdit,
  onDuplicate,
  onDelete,
}: WorkflowCardProps) {
  const displayStatus = getWorkflowDisplayStatus(workflow);
  const description = getWorkflowDescription(workflow.definition);
  const triggerSummary = getWorkflowTriggerSummary(workflow.definition);

  return (
    <div
      className="relative w-full rounded-lg border bg-card p-3 text-left transition-colors hover:bg-muted/50"
      data-testid={`workflow-card-${workflow.id}`}
    >
      <button
        className="absolute inset-0 rounded-lg"
        onClick={() => onSelect(workflow.id)}
        type="button"
      >
        <span className="sr-only">View {workflow.name}</span>
      </button>

      <div className="flex items-start justify-between">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <Zap className="h-4 w-4 shrink-0 text-amber-500" />
            <span className="truncate text-sm font-medium">
              {workflow.name}
            </span>
            <StatusBadge status={displayStatus} />
          </div>
          <div className="mt-1.5 flex items-center gap-3 pl-6 text-[11px] text-muted-foreground">
            {channelName ? <span>{channelName}</span> : null}
            {triggerSummary ? <span>{triggerSummary}</span> : null}
            <span className="flex items-center gap-1">
              <Clock className="h-3 w-3" />
              {new Date(workflow.updatedAt * 1000).toLocaleDateString()}
            </span>
          </div>
          {description ? (
            <p className="mt-2 pl-6 text-xs text-muted-foreground">
              {description}
            </p>
          ) : null}
        </div>

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              aria-label="Workflow actions"
              className="relative z-10 h-7 w-7 shrink-0"
              size="icon"
              variant="ghost"
            >
              <MoreHorizontal className="h-4 w-4" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={() => onTrigger(workflow.id)}>
              <Play className="mr-2 h-4 w-4" />
              Trigger
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => onEdit(workflow)}>
              <Pencil className="mr-2 h-4 w-4" />
              Edit
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => onDuplicate(workflow)}>
              <Copy className="mr-2 h-4 w-4" />
              Duplicate
            </DropdownMenuItem>
            <DropdownMenuItem
              className="text-destructive"
              onClick={() => onDelete(workflow)}
            >
              <Trash2 className="mr-2 h-4 w-4" />
              Delete
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  );
}
