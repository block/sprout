import { Bot, Minus, Plus } from "lucide-react";

import type { AgentPersona } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { promptPreview } from "@/shared/lib/promptPreview";
import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/shared/ui/tooltip";

// ---------------------------------------------------------------------------
// PersonaChip — pill with e-commerce stepper for multi-instance selection
// ---------------------------------------------------------------------------

function PersonaChip({
  avatarUrl,
  count,
  disabled,
  label,
  onSetCount,
  children,
}: {
  avatarUrl?: string | null;
  count: number;
  disabled: boolean;
  label: string;
  onSetCount: (count: number) => void;
  children: React.ReactNode;
}) {
  const showAvatar = avatarUrl !== undefined;
  const isSelected = count > 0;

  if (!isSelected) {
    // Unselected state — single click to add.
    return (
      <button
        aria-label={`Add ${label}`}
        className={cn(
          "inline-flex min-h-9 items-center gap-2 rounded-full border py-1.5 text-sm font-medium transition-colors",
          showAvatar ? "pl-1.5 pr-3" : "px-3",
          "border-border/70 bg-muted/25 text-foreground hover:bg-muted/55",
          disabled && "cursor-not-allowed opacity-50",
        )}
        disabled={disabled}
        onClick={() => onSetCount(1)}
        type="button"
      >
        {showAvatar ? (
          <ProfileAvatar
            avatarUrl={avatarUrl}
            className="h-6 w-6 rounded-full text-[10px] bg-primary/20 text-primary"
            iconClassName="h-3.5 w-3.5"
            label={label}
          />
        ) : null}
        {children}
      </button>
    );
  }

  // Selected state — stepper mode.
  return (
    <div
      className={cn(
        "inline-flex min-h-9 items-center gap-1.5 rounded-full border py-1 text-sm font-medium transition-colors",
        showAvatar ? "pl-1.5 pr-1" : "pl-3 pr-1",
        "border-foreground bg-foreground text-background shadow-sm",
        disabled && "cursor-not-allowed opacity-50",
      )}
    >
      {showAvatar ? (
        <ProfileAvatar
          avatarUrl={avatarUrl}
          className="h-6 w-6 rounded-full text-[10px] bg-background/20 text-background"
          iconClassName="h-3.5 w-3.5"
          label={label}
        />
      ) : null}
      <span className="pr-0.5">{children}</span>
      <span className="inline-flex items-center gap-0.5 rounded-full bg-background/20 px-0.5">
        <button
          aria-label={count === 1 ? `Remove ${label}` : `Decrease ${label}`}
          className="flex h-6 w-6 items-center justify-center rounded-full transition-colors hover:bg-background/20 disabled:opacity-50"
          disabled={disabled}
          onClick={() => onSetCount(count - 1)}
          type="button"
        >
          <Minus className="h-3 w-3" />
        </button>
        <span className="min-w-[1.25rem] text-center text-xs font-bold tabular-nums">
          {count}
        </span>
        <button
          aria-label={`Increase ${label}`}
          className="flex h-6 w-6 items-center justify-center rounded-full transition-colors hover:bg-background/20 disabled:opacity-50"
          disabled={disabled}
          onClick={() => onSetCount(count + 1)}
          type="button"
        >
          <Plus className="h-3 w-3" />
        </button>
      </span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// GenericChip — simple toggle (no stepper, always 0 or 1)
// ---------------------------------------------------------------------------

function GenericChip({
  disabled,
  onClick,
  selected,
}: {
  disabled: boolean;
  onClick: () => void;
  selected: boolean;
}) {
  return (
    <button
      aria-pressed={selected}
      className={cn(
        "inline-flex min-h-9 items-center gap-2 rounded-full border py-1.5 px-3 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        selected
          ? "border-primary bg-primary/10 text-foreground"
          : "border-border/80 bg-background/60 text-muted-foreground hover:bg-accent hover:text-accent-foreground",
        disabled && "cursor-not-allowed opacity-50",
      )}
      disabled={disabled}
      onClick={onClick}
      type="button"
    >
      <Bot
        className={cn(
          "h-4 w-4",
          selected ? "text-background/70" : "text-muted-foreground",
        )}
      />
      Generic
    </button>
  );
}

// ---------------------------------------------------------------------------
// AddChannelBotPersonasSection
// ---------------------------------------------------------------------------

type AddChannelBotPersonasSectionProps = {
  canToggleSelections: boolean;
  inChannelPersonaCounts?: ReadonlyMap<string, number>;
  includeGeneric: boolean;
  isLoading: boolean;
  onToggleGeneric: () => void;
  onSetPersonaCount: (personaId: string, count: number) => void;
  personas: AgentPersona[];
  selectedPersonaCounts: ReadonlyMap<string, number>;
};

export function AddChannelBotPersonasSection({
  canToggleSelections,
  inChannelPersonaCounts,
  includeGeneric,
  isLoading,
  onToggleGeneric,
  onSetPersonaCount,
  personas,
  selectedPersonaCounts,
}: AddChannelBotPersonasSectionProps) {
  return (
    <div className="space-y-3">
      <div className="space-y-3">
        <div>
          <div className="text-sm font-medium">Personas</div>
          <p className="text-xs text-muted-foreground">
            Click to add. Use the stepper to add multiple instances of the same
            persona. Hover to preview its role.
          </p>
        </div>

        <TooltipProvider delayDuration={150}>
          <div className="flex flex-wrap gap-2">
            <Tooltip>
              <TooltipTrigger asChild>
                <div>
                  <GenericChip
                    disabled={!canToggleSelections}
                    onClick={onToggleGeneric}
                    selected={includeGeneric}
                  />
                </div>
              </TooltipTrigger>
              <TooltipContent className="max-w-xs text-left">
                Add one custom agent with a channel-specific name and prompt.
              </TooltipContent>
            </Tooltip>
            {personas.map((persona) => {
              const count = selectedPersonaCounts.get(persona.id) ?? 0;
              const inChannelCount =
                inChannelPersonaCounts?.get(persona.id) ?? 0;
              return (
                <Tooltip key={persona.id}>
                  <TooltipTrigger asChild>
                    <div>
                      <PersonaChip
                        avatarUrl={persona.avatarUrl}
                        count={count}
                        disabled={!canToggleSelections}
                        label={persona.displayName}
                        onSetCount={(newCount) =>
                          onSetPersonaCount(persona.id, Math.max(0, newCount))
                        }
                      >
                        {persona.displayName}
                        {inChannelCount > 0 && count === 0 ? (
                          <span className="inline-flex items-center gap-0.5 rounded-full bg-muted/60 px-1.5 py-0.5 text-[10px] font-medium leading-none text-muted-foreground">
                            {inChannelCount} in channel
                          </span>
                        ) : null}
                      </PersonaChip>
                    </div>
                  </TooltipTrigger>
                  <TooltipContent className="max-w-xs text-left">
                    <div className="space-y-2">
                      <div className="flex items-center gap-2">
                        <ProfileAvatar
                          avatarUrl={persona.avatarUrl}
                          className="h-7 w-7 rounded-full text-[10px] bg-primary-foreground/20 text-primary-foreground"
                          iconClassName="h-3.5 w-3.5"
                          label={persona.displayName}
                        />
                        <p className="font-medium">{persona.displayName}</p>
                      </div>
                      {inChannelCount > 0 ? (
                        <p className="text-[11px] font-medium text-emerald-300">
                          {inChannelCount}{" "}
                          {inChannelCount === 1 ? "instance" : "instances"} in
                          this channel
                        </p>
                      ) : null}
                      <p className="text-[11px] text-primary-foreground">
                        {promptPreview(persona.systemPrompt)}
                      </p>
                    </div>
                  </TooltipContent>
                </Tooltip>
              );
            })}
          </div>
        </TooltipProvider>

        {isLoading ? (
          <p className="text-xs text-muted-foreground">Loading personas...</p>
        ) : null}
      </div>
    </div>
  );
}
