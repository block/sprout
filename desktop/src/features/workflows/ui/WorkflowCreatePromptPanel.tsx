import { Sparkles } from "lucide-react";

import { Button } from "@/shared/ui/button";
import { WorkflowPromptBuilder } from "./WorkflowPromptBuilder";

type WorkflowCreatePromptPanelProps = {
  disabled?: boolean;
  onCancel: () => void;
  onChange: (value: string) => void;
  onSubmit: () => void;
  prompt: string;
};

export function WorkflowCreatePromptPanel({
  disabled,
  onCancel,
  onChange,
  onSubmit,
  prompt,
}: WorkflowCreatePromptPanelProps) {
  return (
    <div
      className="relative flex min-h-0 flex-1 overflow-hidden bg-background"
      data-testid="workflow-create-prompt-panel"
    >
      <div
        aria-hidden="true"
        className="absolute inset-0"
        style={{
          backgroundImage:
            "radial-gradient(hsl(var(--border) / 0.3) 0.7px, transparent 0.8px)",
          backgroundPosition: "10px 10px",
          backgroundSize: "20px 20px",
        }}
      />

      <div className="absolute inset-x-0 top-4 z-10 flex justify-center px-4 sm:top-6">
        <div className="w-full max-w-2xl rounded-2xl border border-border/70 bg-background/95 shadow-xl backdrop-blur">
          <div className="flex items-center gap-2 border-b border-border/70 px-4 py-3">
            <Sparkles className="h-4 w-4 text-muted-foreground" />
            <h3 className="text-sm font-semibold">Create Workflow</h3>
          </div>

          <div className="px-4 py-4">
            <WorkflowPromptBuilder
              disabled={disabled}
              onChange={onChange}
              prompt={prompt}
            />
          </div>

          <div className="flex items-center justify-end gap-2 border-t border-border/70 px-4 py-3">
            <Button disabled={disabled} onClick={onCancel} type="button" variant="outline">
              Cancel
            </Button>
            <Button
              disabled={!prompt.trim() || disabled}
              onClick={onSubmit}
              type="button"
            >
              Draft workflow
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
