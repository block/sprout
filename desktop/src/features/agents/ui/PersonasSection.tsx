import {
  CopyPlus,
  Download,
  Ellipsis,
  Info,
  Pencil,
  Plus,
  Trash2,
  Upload,
} from "lucide-react";

import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import type { AgentPersona } from "@/shared/api/types";
import { useFileImportZone } from "@/shared/hooks/useFileImportZone";
import { promptPreview } from "@/shared/lib/promptPreview";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { Skeleton } from "@/shared/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";

type PersonasSectionProps = {
  personas: AgentPersona[];
  error: Error | null;
  isLoading: boolean;
  isPending: boolean;
  onCreate: () => void;
  onDuplicate: (persona: AgentPersona) => void;
  onEdit: (persona: AgentPersona) => void;
  onExport: (persona: AgentPersona) => void;
  onDelete: (persona: AgentPersona) => void;
  onImportFile: (fileBytes: number[], fileName: string) => void;
};

export function PersonasSection({
  personas,
  error,
  isLoading,
  isPending,
  onCreate,
  onDuplicate,
  onEdit,
  onExport,
  onDelete,
  onImportFile,
}: PersonasSectionProps) {
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
        <div className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center rounded-xl border-2 border-dashed border-primary/50 bg-primary/5">
          <p className="text-sm font-medium text-primary">
            Drop .persona.json, .persona.png, or .zip to import
          </p>
        </div>
      ) : null}

      <div className="flex items-center justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold tracking-tight">Personas</h3>
          <p className="text-sm text-muted-foreground">
            Reusable agent templates for common roles and prompts. Drop a file
            to import.
          </p>
        </div>
        <div className="flex items-center gap-1">
          <input
            accept=".json,.png,.zip"
            className="hidden"
            onChange={handleFileChange}
            ref={fileInputRef}
            type="file"
          />
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                aria-label="Import persona"
                onClick={openFilePicker}
                type="button"
                variant="ghost"
                size="icon"
              >
                <Upload className="h-4 w-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Import persona</TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                aria-label="Create persona"
                onClick={onCreate}
                type="button"
                variant="ghost"
                size="icon"
              >
                <Plus className="h-4 w-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Create persona</TooltipContent>
          </Tooltip>
        </div>
      </div>

      {isLoading ? (
        <div className="grid gap-3 md:grid-cols-3 xl:grid-cols-4">
          {["first", "second", "third", "fourth"].map((key) => (
            <div
              className="rounded-xl border border-border/70 bg-card/80 p-2 shadow-sm"
              key={key}
            >
              <div className="flex items-center gap-2.5">
                <Skeleton className="h-8 w-8 rounded-lg" />
                <div className="space-y-2">
                  <Skeleton className="h-4 w-24" />
                  <Skeleton className="h-3 w-14 rounded-full" />
                </div>
              </div>
            </div>
          ))}
        </div>
      ) : null}

      {!isLoading && personas.length > 0 ? (
        <div className="grid gap-3 md:grid-cols-3 xl:grid-cols-4">
          {personas.map((persona) => {
            const preview = promptPreview(persona.systemPrompt);

            return (
              <div
                className="rounded-xl border border-border/70 bg-card/80 p-2 shadow-sm"
                key={persona.id}
              >
                <div className="flex items-start justify-between gap-2.5">
                  <div className="flex min-w-0 items-center gap-2.5">
                    <ProfileAvatar
                      avatarUrl={persona.avatarUrl}
                      className="h-8 w-8 rounded-lg text-xs"
                      label={persona.displayName}
                    />
                    <div className="min-w-0">
                      <div className="flex items-center gap-2">
                        <p className="truncate text-sm font-semibold tracking-tight">
                          {persona.displayName}
                        </p>
                        {persona.isBuiltIn ? (
                          <span className="whitespace-nowrap rounded-full bg-muted px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
                            Built-in
                          </span>
                        ) : null}
                        {preview ? (
                          <Tooltip>
                            <TooltipTrigger asChild>
                              <button
                                aria-label="View system prompt"
                                className="flex h-4 w-4 shrink-0 items-center justify-center text-muted-foreground transition-colors hover:text-foreground"
                                type="button"
                              >
                                <Info className="h-3.5 w-3.5" />
                              </button>
                            </TooltipTrigger>
                            <TooltipContent side="bottom" className="max-w-xs">
                              <p>{preview}</p>
                            </TooltipContent>
                          </Tooltip>
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
                      <DropdownMenuItem
                        disabled={isPending}
                        onClick={() => onExport(persona)}
                      >
                        <Download className="h-4 w-4" />
                        Export
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
              </div>
            );
          })}
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
          <p className="mt-1 text-xs text-muted-foreground/70">
            Or drop a .persona.json, .persona.png, or .zip file here to import.
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
