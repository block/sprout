import { Users } from "lucide-react";
import type * as React from "react";

import type { AgentPersona, AgentTeam } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/shared/ui/tooltip";

type SelectionChipButtonProps = {
  disabled: boolean;
  label: string;
  onClick: () => void;
  selected: boolean;
  children: React.ReactNode;
};

function SelectionChipButton({
  disabled,
  label: _label,
  onClick,
  selected,
  children,
}: SelectionChipButtonProps) {
  return (
    <button
      aria-pressed={selected}
      className={cn(
        "inline-flex min-h-9 items-center gap-2 rounded-full border py-1.5 px-3 text-sm font-medium transition-colors",
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

function resolveTeamPersonas(
  team: AgentTeam,
  personas: AgentPersona[],
): AgentPersona[] {
  return team.personaIds
    .map((id) => personas.find((p) => p.id === id))
    .filter((p): p is AgentPersona => p !== undefined);
}

type AddChannelBotTeamsSectionProps = {
  canToggleSelections: boolean;
  isLoading: boolean;
  onToggleTeam: (personaIds: string[]) => void;
  personas: AgentPersona[];
  selectedPersonaIds: readonly string[];
  teams: AgentTeam[];
};

export function AddChannelBotTeamsSection({
  canToggleSelections,
  isLoading,
  onToggleTeam,
  personas,
  selectedPersonaIds,
  teams,
}: AddChannelBotTeamsSectionProps) {
  if (isLoading || teams.length === 0) {
    return null;
  }

  return (
    <div className="space-y-3">
      <div>
        <div className="text-sm font-medium">Teams</div>
        <p className="text-xs text-muted-foreground">
          Select a team to toggle all its personas at once.
        </p>
      </div>

      <TooltipProvider delayDuration={150}>
        <div className="flex flex-wrap gap-2">
          {teams.map((team) => {
            const resolved = resolveTeamPersonas(team, personas);
            const validIds = resolved.map((p) => p.id);
            const allSelected =
              validIds.length > 0 &&
              validIds.every((id) => selectedPersonaIds.includes(id));

            return (
              <Tooltip key={team.id}>
                <TooltipTrigger asChild>
                  <div>
                    <SelectionChipButton
                      disabled={!canToggleSelections || validIds.length === 0}
                      label={team.name}
                      onClick={() => onToggleTeam(validIds)}
                      selected={allSelected}
                    >
                      <Users
                        className={cn(
                          "h-4 w-4",
                          allSelected
                            ? "text-background/70"
                            : "text-muted-foreground",
                        )}
                      />
                      {team.name}
                      <span
                        className={cn(
                          "text-xs",
                          allSelected
                            ? "text-background/60"
                            : "text-muted-foreground",
                        )}
                      >
                        ({validIds.length})
                      </span>
                    </SelectionChipButton>
                  </div>
                </TooltipTrigger>
                <TooltipContent className="max-w-xs text-left">
                  <div className="space-y-1.5">
                    <p className="font-medium">{team.name}</p>
                    {team.description ? (
                      <p className="text-[11px] text-primary-foreground/80">
                        {team.description}
                      </p>
                    ) : null}
                    <div className="flex flex-wrap gap-1">
                      {resolved.map((persona) => (
                        <div
                          className="flex items-center gap-1 rounded-full bg-primary-foreground/10 px-1.5 py-0.5"
                          key={persona.id}
                        >
                          <ProfileAvatar
                            avatarUrl={persona.avatarUrl}
                            className="h-4 w-4 rounded-full text-[8px] bg-primary-foreground/20 text-primary-foreground"
                            label={persona.displayName}
                          />
                          <span className="text-[10px] text-primary-foreground">
                            {persona.displayName}
                          </span>
                        </div>
                      ))}
                    </div>
                  </div>
                </TooltipContent>
              </Tooltip>
            );
          })}
        </div>
      </TooltipProvider>
    </div>
  );
}
