import { Textarea } from "@/shared/ui/textarea";
import { FieldLabel } from "./workflowFormPrimitives";

type WorkflowPromptBuilderProps = {
  disabled?: boolean;
  onChange: (value: string) => void;
  prompt: string;
};

export function WorkflowPromptBuilder({
  disabled,
  onChange,
  prompt,
}: WorkflowPromptBuilderProps) {
  return (
    <div className="space-y-4">
      <div className="space-y-1.5">
        <FieldLabel htmlFor="wf-prompt">Describe the workflow</FieldLabel>
        <Textarea
          autoCapitalize="off"
          className="min-h-[180px] resize-y text-sm"
          disabled={disabled}
          id="wf-prompt"
          onChange={(event) => onChange(event.target.value)}
          placeholder='e.g. When someone posts "deploy", wait 5 minutes and send a message to the channel.'
          value={prompt}
        />
        <p className="text-xs text-muted-foreground">
          We&apos;ll draft the closest workflow we can, then you can adjust the
          trigger, steps, or YAML before saving.
        </p>
      </div>

      <div className="rounded-lg border border-border/70 bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
        Include the trigger, any filters, and what should happen next. Mention
        timings, reactions, webhook URLs, or approval requirements if they
        matter.
      </div>
    </div>
  );
}
