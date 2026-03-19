import {
  CopyPlus,
  Ellipsis,
  Pencil,
  Plus,
  RefreshCcw,
  Trash2,
} from "lucide-react";

import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import type { AgentPersona } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { Skeleton } from "@/shared/ui/skeleton";

function promptPreview(systemPrompt: string) {
  const [firstLine] = systemPrompt
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
  return firstLine ?? systemPrompt.trim();
}

type PersonasSectionProps = {
  personas: AgentPersona[];
  error: Error | null;
  isLoading: boolean;
  isPending: boolean;
  onCreate: () => void;
  onDuplicate: (persona: AgentPersona) => void;
  onEdit: (persona: AgentPersona) => void;
  onDelete: (persona: AgentPersona) => void;
  onRefresh: () => void;
};

export function PersonasSection({
  personas,
  error,
  isLoading,
  isPending,
  onCreate,
  onDuplicate,
  onEdit,
  onDelete,
  onRefresh,
}: PersonasSectionProps) {
  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold tracking-tight">Personas</h3>
          <p className="text-sm text-muted-foreground">
            Reusable agent templates for common roles and prompts.
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button onClick={onCreate} type="button">
            <Plus className="h-4 w-4" />
            Create persona
          </Button>
          <Button onClick={onRefresh} type="button" variant="outline">
            <RefreshCcw className="h-4 w-4" />
            Refresh
          </Button>
        </div>
      </div>

      {isLoading ? (
        <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
          {["first", "second", "third", "fourth"].map((key) => (
            <div
              className="rounded-xl border border-border/70 bg-card/80 p-3 shadow-sm"
              key={key}
            >
              <div className="flex items-center gap-2.5">
                <Skeleton className="h-10 w-10 rounded-lg" />
                <div className="space-y-2">
                  <Skeleton className="h-4 w-24" />
                  <Skeleton className="h-3 w-14 rounded-full" />
                </div>
              </div>
              <Skeleton className="mt-3 h-9 w-full rounded-lg" />
            </div>
          ))}
        </div>
      ) : null}

      {!isLoading && personas.length > 0 ? (
        <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
          {personas.map((persona) => (
            <div
              className="rounded-xl border border-border/70 bg-card/80 p-3 shadow-sm"
              key={persona.id}
            >
              <div className="flex items-start justify-between gap-2.5">
                <div className="flex min-w-0 items-center gap-2.5">
                  <ProfileAvatar
                    avatarUrl={persona.avatarUrl}
                    className="h-10 w-10 rounded-lg text-xs"
                    label={persona.displayName}
                  />
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <p className="truncate text-sm font-semibold tracking-tight">
                        {persona.displayName}
                      </p>
                      {persona.isBuiltIn ? (
                        <span className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
                          Built-in
                        </span>
                      ) : null}
                    </div>
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
                    {!persona.isBuiltIn ? (
                      <DropdownMenuItem
                        disabled={isPending}
                        onClick={() => onEdit(persona)}
                      >
                        <Pencil className="h-4 w-4" />
                        Edit
                      </DropdownMenuItem>
                    ) : null}
                    <DropdownMenuItem
                      disabled={isPending}
                      onClick={() => onDuplicate(persona)}
                    >
                      <CopyPlus className="h-4 w-4" />
                      Duplicate
                    </DropdownMenuItem>
                    {!persona.isBuiltIn ? (
                      <DropdownMenuItem
                        className="text-destructive focus:text-destructive"
                        disabled={isPending}
                        onClick={() => onDelete(persona)}
                      >
                        <Trash2 className="h-4 w-4" />
                        Delete
                      </DropdownMenuItem>
                    ) : null}
                  </DropdownMenuContent>
                </DropdownMenu>
              </div>

              <p className="mt-3 line-clamp-2 text-xs leading-5 text-muted-foreground">
                {promptPreview(persona.systemPrompt)}
              </p>
            </div>
          ))}
        </div>
      ) : null}

      {!isLoading && personas.length === 0 ? (
        <div className="rounded-xl border border-dashed border-border/80 bg-card/70 px-6 py-10 text-center">
          <p className="text-sm font-semibold tracking-tight">
            No personas yet
          </p>
          <p className="mt-2 text-sm text-muted-foreground">
            Create one to save a role, prompt, and optional avatar for reuse.
          </p>
        </div>
      ) : null}

      {error ? (
        <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {error.message}
        </p>
      ) : null}
    </section>
  );
}
