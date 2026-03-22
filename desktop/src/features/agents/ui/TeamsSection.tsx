import {
  CopyPlus,
  Download,
  Ellipsis,
  Info,
  Pencil,
  Plus,
  Rocket,
  Trash2,
  Upload,
  Users,
} from "lucide-react";

import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import type { AgentPersona, AgentTeam } from "@/shared/api/types";
import { useFileImportZone } from "@/shared/hooks/useFileImportZone";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { Skeleton } from "@/shared/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";

const MAX_VISIBLE_AVATARS = 4;

type TeamsSectionProps = {
  teams: AgentTeam[];
  personas: AgentPersona[];
  error: Error | null;
  isLoading: boolean;
  isPending: boolean;
  onCreate: () => void;
  onDuplicate: (team: AgentTeam) => void;
  onEdit: (team: AgentTeam) => void;
  onExport: (team: AgentTeam) => void;
  onDelete: (team: AgentTeam) => void;
  onAddToChannel: (team: AgentTeam) => void;
  onImportFile: (fileBytes: number[], fileName: string) => void;
};

function resolvePersonas(
  personaIds: string[],
  personas: AgentPersona[],
): AgentPersona[] {
  return personaIds
    .map((id) => personas.find((p) => p.id === id))
    .filter((p): p is AgentPersona => p !== undefined);
}

export function TeamsSection({
  teams,
  personas,
  error,
  isLoading,
  isPending,
  onCreate,
  onDuplicate,
  onEdit,
  onExport,
  onDelete,
  onAddToChannel,
  onImportFile,
}: TeamsSectionProps) {
  const {
    fileInputRef,
    isDragOver,
    dropHandlers,
    handleFileChange,
    openFilePicker,
  } = useFileImportZone({ onImportFile });

  return (
    <section className="relative space-y-4" {...dropHandlers}>
      {isDragOver ? (
        <div className="pointer-events-none absolute -inset-2 z-10 flex items-center justify-center rounded-2xl border-2 border-dashed border-primary/50 bg-primary/5">
          <p className="text-sm font-medium text-primary">
            Drop .team.json to import
          </p>
        </div>
      ) : null}

      <div className="flex items-center justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold tracking-tight">Teams</h3>
          <p className="text-sm text-muted-foreground">
            Named groups of personas you can deploy to a channel together.
          </p>
        </div>
        <input
          accept=".json"
          className="hidden"
          onChange={handleFileChange}
          ref={fileInputRef}
          type="file"
        />
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              aria-label="Create team"
              onClick={onCreate}
              type="button"
              variant="ghost"
              size="icon"
            >
              <Plus className="h-4 w-4" />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Create team</TooltipContent>
        </Tooltip>
      </div>

      {isLoading ? (
        <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
          {["first", "second", "third"].map((key) => (
            <div
              className="rounded-xl border border-border/70 bg-card/80 p-3 shadow-sm"
              key={key}
            >
              <div className="flex items-center gap-2.5">
                <Skeleton className="h-8 w-8 rounded-lg" />
                <div className="space-y-2">
                  <Skeleton className="h-4 w-32" />
                  <Skeleton className="h-3 w-20 rounded-full" />
                </div>
              </div>
            </div>
          ))}
        </div>
      ) : null}

      {!isLoading && teams.length > 0 ? (
        <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
          {teams.map((team) => {
            const resolved = resolvePersonas(team.personaIds, personas);
            const visible = resolved.slice(0, MAX_VISIBLE_AVATARS);
            const overflow = resolved.length - visible.length;

            return (
              <div
                className="rounded-xl border border-border/70 bg-card/80 p-3 shadow-sm"
                key={team.id}
              >
                <div className="flex items-start justify-between gap-2.5">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <Users className="h-4 w-4 shrink-0 text-muted-foreground" />
                      <p className="truncate text-sm font-semibold tracking-tight">
                        {team.name}
                      </p>
                      {team.description ? (
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <button
                              aria-label="View description"
                              className="flex h-4 w-4 shrink-0 items-center justify-center text-muted-foreground transition-colors hover:text-foreground"
                              type="button"
                            >
                              <Info className="h-3.5 w-3.5" />
                            </button>
                          </TooltipTrigger>
                          <TooltipContent side="bottom" className="max-w-xs">
                            <p>{team.description}</p>
                          </TooltipContent>
                        </Tooltip>
                      ) : null}
                    </div>

                    <div className="mt-2 flex items-center gap-2">
                      <div className="flex -space-x-1.5">
                        {visible.map((persona) => (
                          <ProfileAvatar
                            avatarUrl={persona.avatarUrl}
                            className="h-6 w-6 rounded-full border-2 border-card text-[10px]"
                            key={persona.id}
                            label={persona.displayName}
                          />
                        ))}
                        {overflow > 0 ? (
                          <span className="flex h-6 w-6 items-center justify-center rounded-full border-2 border-card bg-muted text-[10px] font-medium text-muted-foreground">
                            +{overflow}
                          </span>
                        ) : null}
                      </div>
                      <span className="text-xs text-muted-foreground">
                        {team.personaIds.length}{" "}
                        {team.personaIds.length === 1 ? "persona" : "personas"}
                      </span>
                    </div>
                  </div>

                  <DropdownMenu modal={false}>
                    <DropdownMenuTrigger asChild>
                      <button
                        className="flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                        type="button"
                      >
                        <Ellipsis className="h-4 w-4" />
                      </button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent
                      align="end"
                      onCloseAutoFocus={(event) => event.preventDefault()}
                    >
                      <DropdownMenuItem
                        disabled={isPending}
                        onClick={() => onAddToChannel(team)}
                      >
                        <Rocket className="h-4 w-4" />
                        Deploy to channel
                      </DropdownMenuItem>
                      <DropdownMenuSeparator />
                      <DropdownMenuItem
                        disabled={isPending}
                        onClick={() => onEdit(team)}
                      >
                        <Pencil className="h-4 w-4" />
                        Edit
                      </DropdownMenuItem>
                      <DropdownMenuItem
                        disabled={isPending}
                        onClick={() => onDuplicate(team)}
                      >
                        <CopyPlus className="h-4 w-4" />
                        Duplicate
                      </DropdownMenuItem>
                      <DropdownMenuItem
                        disabled={isPending}
                        onClick={() => onExport(team)}
                      >
                        <Download className="h-4 w-4" />
                        Export
                      </DropdownMenuItem>
                      <DropdownMenuSeparator />
                      <DropdownMenuItem
                        className="text-destructive focus:text-destructive"
                        disabled={isPending}
                        onClick={() => onDelete(team)}
                      >
                        <Trash2 className="h-4 w-4" />
                        Delete
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
              </div>
            );
          })}
          <button
            className="flex cursor-pointer items-center justify-center gap-2 rounded-xl border border-dashed border-primary/30 bg-primary/[0.02] p-3 text-primary/50 transition-colors hover:border-primary/50 hover:bg-primary/5 hover:text-primary/70"
            onClick={openFilePicker}
            type="button"
          >
            <Upload className="h-4 w-4" />
            <span className="text-xs">Import</span>
          </button>
        </div>
      ) : null}

      {!isLoading && teams.length === 0 ? (
        <button
          className="w-full cursor-pointer rounded-xl border border-dashed border-primary/20 bg-primary/[0.02] px-6 py-10 text-center transition-colors hover:border-primary/40 hover:bg-primary/5"
          onClick={openFilePicker}
          type="button"
        >
          <p className="text-sm font-semibold tracking-tight">No teams yet</p>
          <p className="mt-2 text-sm text-muted-foreground">
            Create a team to group personas for quick deployment to channels.
          </p>
          <p className="mt-1 text-xs text-muted-foreground/70">
            Or drop a .team.json file here to import.
          </p>
        </button>
      ) : null}

      {error ? (
        <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {error.message}
        </p>
      ) : null}
    </section>
  );
}
