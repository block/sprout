import type { AgentPersona } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/shared/ui/tooltip";

function promptPreview(prompt: string) {
  const [firstLine] = prompt
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);

  return firstLine ?? prompt.trim();
}

type SelectionChipButtonProps = {
  disabled: boolean;
  onClick: () => void;
  selected: boolean;
  children: React.ReactNode;
};

function SelectionChipButton({
  disabled,
  onClick,
  selected,
  children,
}: SelectionChipButtonProps) {
  return (
    <button
      aria-pressed={selected}
      className={cn(
        "inline-flex min-h-9 items-center rounded-full border px-3 py-1.5 text-sm font-medium transition-colors",
        selected
          ? "border-foreground bg-foreground text-background shadow-sm"
          : "border-border/70 bg-muted/25 text-foreground hover:bg-muted/55",
        disabled && "cursor-not-allowed opacity-50",
      )}
      disabled={disabled}
      onClick={onClick}
      type="button"
    >
      {children}
    </button>
  );
}

type AddChannelBotPersonasSectionProps = {
  canToggleSelections: boolean;
  includeGeneric: boolean;
  isLoading: boolean;
  onToggleGeneric: () => void;
  onTogglePersona: (personaId: string) => void;
  personas: AgentPersona[];
  selectedPersonaIds: readonly string[];
};

export function AddChannelBotPersonasSection({
  canToggleSelections,
  includeGeneric,
  isLoading,
  onToggleGeneric,
  onTogglePersona,
  personas,
  selectedPersonaIds,
}: AddChannelBotPersonasSectionProps) {
  return (
    <div className="space-y-3">
      <div className="space-y-3">
        <div>
          <div className="text-sm font-medium">Personas</div>
          <p className="text-xs text-muted-foreground">
            Toggle as many as you want. Each selected persona is added as its
            own agent. Hover a persona to preview its role.
          </p>
        </div>

        <TooltipProvider delayDuration={150}>
          <div className="flex flex-wrap gap-2">
            <Tooltip>
              <TooltipTrigger asChild>
                <div>
                  <SelectionChipButton
                    disabled={!canToggleSelections}
                    onClick={onToggleGeneric}
                    selected={includeGeneric}
                  >
                    Generic
                  </SelectionChipButton>
                </div>
              </TooltipTrigger>
              <TooltipContent className="max-w-xs text-left">
                Add one custom agent with a channel-specific name and prompt.
              </TooltipContent>
            </Tooltip>
            {personas.map((persona) => (
              <Tooltip key={persona.id}>
                <TooltipTrigger asChild>
                  <div>
                    <SelectionChipButton
                      disabled={!canToggleSelections}
                      onClick={() => onTogglePersona(persona.id)}
                      selected={selectedPersonaIds.includes(persona.id)}
                    >
                      {persona.displayName}
                    </SelectionChipButton>
                  </div>
                </TooltipTrigger>
                <TooltipContent className="max-w-xs text-left">
                  <div className="space-y-1">
                    <p className="font-medium">{persona.displayName}</p>
                    <p>{promptPreview(persona.systemPrompt)}</p>
                  </div>
                </TooltipContent>
              </Tooltip>
            ))}
          </div>
        </TooltipProvider>

        {isLoading ? (
          <p className="text-xs text-muted-foreground">Loading personas...</p>
        ) : null}
      </div>
    </div>
  );
}
