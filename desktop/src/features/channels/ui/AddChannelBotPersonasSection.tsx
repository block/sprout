import { Bot, Check } from "lucide-react";

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

type SelectionChipButtonProps = {
  avatarUrl?: string | null;
  disabled: boolean;
  label: string;
  onClick: () => void;
  selected: boolean;
  children: React.ReactNode;
};

function SelectionChipButton({
  avatarUrl,
  disabled,
  label,
  onClick,
  selected,
  children,
}: SelectionChipButtonProps) {
  const showAvatar = avatarUrl !== undefined;

  return (
    <button
      aria-pressed={selected}
      className={cn(
        "inline-flex min-h-9 items-center gap-2 rounded-full border py-1.5 text-sm font-medium transition-colors",
        showAvatar ? "pl-1.5 pr-3" : "px-3",
        selected
          ? "border-foreground bg-foreground text-background shadow-sm"
          : "border-border/70 bg-muted/25 text-foreground hover:bg-muted/55",
        disabled && "cursor-not-allowed opacity-50",
      )}
      disabled={disabled}
      onClick={onClick}
      type="button"
    >
      {showAvatar ? (
        <ProfileAvatar
          avatarUrl={avatarUrl}
          className={cn(
            "h-6 w-6 rounded-full text-[10px]",
            selected
              ? "bg-background/20 text-background"
              : "bg-primary/20 text-primary",
          )}
          iconClassName="h-3.5 w-3.5"
          label={label}
        />
      ) : null}
      {children}
    </button>
  );
}

type AddChannelBotPersonasSectionProps = {
  canToggleSelections: boolean;
  inChannelPersonaIds?: ReadonlySet<string>;
  includeGeneric: boolean;
  isLoading: boolean;
  onToggleGeneric: () => void;
  onTogglePersona: (personaId: string) => void;
  personas: AgentPersona[];
  selectedPersonaIds: readonly string[];
};

export function AddChannelBotPersonasSection({
  canToggleSelections,
  inChannelPersonaIds,
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
                    label="Generic"
                    onClick={onToggleGeneric}
                    selected={includeGeneric}
                  >
                    <Bot
                      className={cn(
                        "h-4 w-4",
                        includeGeneric
                          ? "text-background/70"
                          : "text-muted-foreground",
                      )}
                    />
                    Generic
                  </SelectionChipButton>
                </div>
              </TooltipTrigger>
              <TooltipContent className="max-w-xs text-left">
                Add one custom agent with a channel-specific name and prompt.
              </TooltipContent>
            </Tooltip>
            {personas.map((persona) => {
              const isSelected = selectedPersonaIds.includes(persona.id);
              const isInChannel = inChannelPersonaIds?.has(persona.id) ?? false;
              return (
                <Tooltip key={persona.id}>
                  <TooltipTrigger asChild>
                    <div>
                      <SelectionChipButton
                        avatarUrl={persona.avatarUrl}
                        disabled={!canToggleSelections}
                        label={persona.displayName}
                        onClick={() => onTogglePersona(persona.id)}
                        selected={isSelected}
                      >
                        {persona.displayName}
                        {isInChannel ? (
                          <span
                            className={cn(
                              "inline-flex items-center gap-0.5 rounded-full px-1.5 py-0.5 text-[10px] font-medium leading-none",
                              isSelected
                                ? "bg-background/20 text-background/80"
                                : "bg-muted/60 text-muted-foreground",
                            )}
                          >
                            <Check className="h-2.5 w-2.5" />
                            In channel
                          </span>
                        ) : null}
                      </SelectionChipButton>
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
                      {isInChannel ? (
                        <p className="text-[11px] font-medium text-emerald-300">
                          ✓ Already in this channel
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
