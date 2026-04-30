import {
  CopyPlus,
  Download,
  Ellipsis,
  Pencil,
  Trash2,
  Upload,
} from "lucide-react";

import type { AgentPersona } from "@/shared/api/types";
import { useFeedbackToasts } from "@/shared/hooks/useToastEffect";
import { useFileImportZone } from "@/shared/hooks/useFileImportZone";
import { Card } from "@/shared/ui/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { Skeleton } from "@/shared/ui/skeleton";
import { PersonaIdentity } from "./PersonaIdentity";
import { PersonaLibraryEntryPoints } from "./PersonaLibraryEntryPoints";
import { personaLibraryCopy } from "./personaLibraryCopy";

type PersonasSectionProps = {
  canChooseCatalog: boolean;
  personas: AgentPersona[];
  error: Error | null;
  feedbackErrorMessage: string | null;
  feedbackNoticeMessage: string | null;
  isLoading: boolean;
  isPending: boolean;
  onCreate: () => void;
  onChooseCatalog: () => void;
  onDuplicate: (persona: AgentPersona) => void;
  onEdit: (persona: AgentPersona) => void;
  onExport: (persona: AgentPersona) => void;
  onDeactivate: (persona: AgentPersona) => void;
  onDelete: (persona: AgentPersona) => void;
  onImportFile: (fileBytes: number[], fileName: string) => void;
};

export function PersonasSection({
  canChooseCatalog,
  personas,
  error,
  feedbackErrorMessage,
  feedbackNoticeMessage,
  isLoading,
  isPending,
  onCreate,
  onChooseCatalog,
  onDuplicate,
  onEdit,
  onExport,
  onDeactivate,
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

  useFeedbackToasts(feedbackNoticeMessage, feedbackErrorMessage);

  return (
    <section
      className="relative space-y-4"
      data-testid="agents-library-personas"
      {...dropHandlers}
    >
      {isDragOver ? (
        <div className="pointer-events-none absolute -inset-1 z-10 flex items-center justify-center rounded-2xl border-2 border-dashed border-primary/50 bg-background/80 backdrop-blur-sm">
          <p className="text-sm font-medium text-primary">
            Drop .persona.md, .persona.json, .persona.png, or .zip to import
          </p>
        </div>
      ) : null}

      <div className="flex items-center justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold tracking-tight">
            {personaLibraryCopy.title}
          </h3>
          <p className="text-sm text-muted-foreground">
            {personaLibraryCopy.description}
          </p>
        </div>
        <input
          accept=".md,.json,.png,.zip"
          className="hidden"
          onChange={handleFileChange}
          ref={fileInputRef}
          type="file"
        />
        <div className="flex items-center gap-2">
          <PersonaLibraryEntryPoints
            canChooseCatalog={canChooseCatalog && personas.length > 0}
            isPending={isPending}
            layout="header"
            onCreate={onCreate}
            onChooseCatalog={onChooseCatalog}
          />
        </div>
      </div>

      {isLoading ? (
        <div className="grid gap-3 md:grid-cols-3 xl:grid-cols-4">
          {["first", "second", "third", "fourth"].map((key) => (
            <Card className="p-2" key={key}>
              <div className="flex items-center gap-2.5">
                <Skeleton className="h-8 w-8 rounded-lg" />
                <div className="space-y-2">
                  <Skeleton className="h-4 w-24" />
                  <Skeleton className="h-3 w-14 rounded-full" />
                </div>
              </div>
            </Card>
          ))}
        </div>
      ) : null}

      {!isLoading && personas.length > 0 ? (
        <div className="grid gap-3 md:grid-cols-3 xl:grid-cols-4">
          {personas.map((persona) => (
            <Card
              className="overflow-hidden p-2"
              data-testid={`library-persona-${persona.id}`}
              key={persona.id}
            >
              <div className="flex items-start justify-between gap-2.5">
                <PersonaIdentity persona={persona} />

                <DropdownMenu modal={false}>
                  <DropdownMenuTrigger asChild>
                    <button
                      aria-label={`Open actions for ${persona.displayName}`}
                      className="flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                      data-testid={`library-persona-menu-${persona.id}`}
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
                    {persona.isBuiltIn ? (
                      <DropdownMenuItem
                        className="text-destructive focus:text-destructive"
                        disabled={isPending}
                        onClick={() => onDeactivate(persona)}
                      >
                        <Trash2 className="h-4 w-4" />
                        Remove from My Agents
                      </DropdownMenuItem>
                    ) : persona.sourcePack ? (
                      <DropdownMenuItem disabled>
                        <Trash2 className="h-4 w-4" />
                        Managed by pack
                      </DropdownMenuItem>
                    ) : (
                      <DropdownMenuItem
                        className="text-destructive focus:text-destructive"
                        disabled={isPending}
                        onClick={() => onDelete(persona)}
                      >
                        <Trash2 className="h-4 w-4" />
                        Delete
                      </DropdownMenuItem>
                    )}
                  </DropdownMenuContent>
                </DropdownMenu>
              </div>
            </Card>
          ))}
          <button
            className="flex cursor-pointer items-center justify-center gap-2 rounded-xl border border-dashed border-primary p-2 text-primary transition-colors hover:bg-primary/5"
            onClick={openFilePicker}
            type="button"
          >
            <Upload className="h-4 w-4" />
            <span className="text-xs">Import</span>
          </button>
        </div>
      ) : null}

      {!isLoading && personas.length === 0 ? (
        <div className="rounded-xl border border-dashed border-primary/40 px-6 py-10 text-center">
          <p className="text-sm font-semibold tracking-tight">
            {personaLibraryCopy.emptyTitle}
          </p>
          <p className="mt-2 text-sm text-muted-foreground">
            {personaLibraryCopy.emptyDescription}
          </p>
          <p className="mt-1 text-xs text-muted-foreground/70">
            {personaLibraryCopy.emptyImportHint}
          </p>
          <div className="mt-4 flex flex-wrap items-center justify-center gap-2">
            <PersonaLibraryEntryPoints
              canChooseCatalog={canChooseCatalog}
              isPending={isPending}
              layout="empty"
              onCreate={onCreate}
              onChooseCatalog={onChooseCatalog}
              onImport={openFilePicker}
            />
          </div>
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
