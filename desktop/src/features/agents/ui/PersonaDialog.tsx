import * as React from "react";
import { Loader2, Upload } from "lucide-react";

import type { ParsePersonaFilesResult } from "@/shared/api/tauriPersonas";
import { parsePersonaFiles } from "@/shared/api/tauriPersonas";
import type {
  CreatePersonaInput,
  UpdatePersonaInput,
} from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";
import { Textarea } from "@/shared/ui/textarea";

const MAX_FILE_SIZE = 100 * 1024 * 1024; // 100 MB (ZIP ceiling)
const PNG_MAGIC = [0x89, 0x50, 0x4e, 0x47];
const ZIP_MAGIC = [0x50, 0x4b, 0x03, 0x04];
const JSON_FIRST_BYTE = 0x7b; // '{'

function matchesMagic(bytes: number[], magic: number[]) {
  return magic.every((b, i) => bytes[i] === b);
}

type PersonaDialogProps = {
  open: boolean;
  title: string;
  description: string;
  submitLabel: string;
  initialValues: CreatePersonaInput | UpdatePersonaInput | null;
  error: Error | null;
  isPending: boolean;
  enableImportDrop?: boolean;
  onOpenChange: (open: boolean) => void;
  onSubmit: (input: CreatePersonaInput | UpdatePersonaInput) => Promise<void>;
  onBatchImport?: (result: ParsePersonaFilesResult, fileName: string) => void;
};

export function PersonaDialog({
  open,
  title,
  description,
  submitLabel,
  initialValues,
  error,
  isPending,
  enableImportDrop,
  onOpenChange,
  onSubmit,
  onBatchImport,
}: PersonaDialogProps) {
  const [displayName, setDisplayName] = React.useState("");
  const [avatarUrl, setAvatarUrl] = React.useState("");
  const [systemPrompt, setSystemPrompt] = React.useState("");
  const [isDragOver, setIsDragOver] = React.useState(false);
  const [isParsing, setIsParsing] = React.useState(false);
  const [importError, setImportError] = React.useState<string | null>(null);

  React.useEffect(() => {
    if (!open || !initialValues) {
      return;
    }

    setDisplayName(initialValues.displayName);
    setAvatarUrl(initialValues.avatarUrl ?? "");
    setSystemPrompt(initialValues.systemPrompt);
  }, [initialValues, open]);

  function handleOpenChange(next: boolean) {
    if (!next) {
      setDisplayName("");
      setAvatarUrl("");
      setSystemPrompt("");
      setIsDragOver(false);
      setIsParsing(false);
      setImportError(null);
    }

    onOpenChange(next);
  }

  async function handleDrop(e: React.DragEvent) {
    e.preventDefault();
    setIsDragOver(false);

    if (!enableImportDrop) {
      return;
    }

    const file = e.dataTransfer.files[0];
    if (!file) {
      return;
    }

    if (file.size > MAX_FILE_SIZE) {
      setImportError("File is too large (max 100 MB).");
      return;
    }

    setImportError(null);
    setIsParsing(true);

    try {
      const buffer = await file.arrayBuffer();
      const bytes = Array.from(new Uint8Array(buffer));
      const result = await parsePersonaFiles(bytes, file.name);

      const isPng = matchesMagic(bytes, PNG_MAGIC);
      const isZip = matchesMagic(bytes, ZIP_MAGIC);
      const isJson = bytes.length > 0 && bytes[0] === JSON_FIRST_BYTE;

      if ((isPng || isJson) && result.personas.length === 1) {
        const persona = result.personas[0];
        setDisplayName(persona.displayName);
        setSystemPrompt(persona.systemPrompt);
        setAvatarUrl(persona.avatarDataUrl ?? "");
        return;
      }

      if (isZip && result.personas.length > 0 && onBatchImport) {
        onBatchImport(result, file.name);
        return;
      }

      if (result.personas.length === 0) {
        setImportError("No valid personas found in file.");
        return;
      }

      setImportError(
        "Unsupported file format. Drop a .persona.json, .persona.png, or .zip.",
      );
    } catch (err) {
      setImportError(
        err instanceof Error ? err.message : "Failed to parse file.",
      );
    } finally {
      setIsParsing(false);
    }
  }

  async function handleSubmit() {
    if (!initialValues) {
      return;
    }

    const baseInput = {
      displayName,
      avatarUrl: avatarUrl.trim() || undefined,
      systemPrompt,
    };

    if ("id" in initialValues) {
      await onSubmit({
        id: initialValues.id,
        ...baseInput,
      });
      return;
    }

    await onSubmit(baseInput);
  }

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent
        className="max-w-2xl overflow-hidden p-0"
        onDragLeave={() => setIsDragOver(false)}
        onDragOver={(e: React.DragEvent) => {
          if (enableImportDrop) {
            e.preventDefault();
            setIsDragOver(true);
          }
        }}
        onDrop={(e: React.DragEvent) => void handleDrop(e)}
      >
        <div className="relative flex max-h-[85vh] flex-col">
          {isParsing ? (
            <div className="pointer-events-none absolute inset-0 z-50 flex items-center justify-center rounded-lg bg-background/80">
              <div className="flex items-center gap-2">
                <Loader2 className="h-4 w-4 animate-spin text-primary" />
                <p className="text-sm font-medium text-primary">
                  Parsing file...
                </p>
              </div>
            </div>
          ) : isDragOver && enableImportDrop ? (
            <div className="pointer-events-none absolute inset-0 z-50 flex items-center justify-center rounded-lg border-2 border-dashed border-primary/50 bg-primary/5">
              <p className="text-sm font-medium text-primary">
                Drop .persona.json, .persona.png, or .zip
              </p>
            </div>
          ) : null}

          <DialogHeader className="border-b border-border/60 px-6 py-5 pr-14">
            <DialogTitle>{title}</DialogTitle>
            <DialogDescription>
              {description}
              {enableImportDrop ? (
                <span className="mt-1 block text-xs text-muted-foreground/70">
                  Or drag a .persona.json, .persona.png, or .zip onto this
                  dialog to import.
                </span>
              ) : null}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-5 px-6 py-5">
            <div className="space-y-1.5">
              <label
                className="text-sm font-medium"
                htmlFor="persona-display-name"
              >
                Display name
              </label>
              <Input
                autoCorrect="off"
                disabled={isPending}
                id="persona-display-name"
                onChange={(event) => setDisplayName(event.target.value)}
                placeholder="Researcher"
                value={displayName}
              />
            </div>

            <div className="space-y-1.5">
              <label
                className="text-sm font-medium"
                htmlFor="persona-avatar-url"
              >
                Avatar URL
              </label>
              <Input
                autoCapitalize="none"
                autoCorrect="off"
                disabled={isPending}
                id="persona-avatar-url"
                onChange={(event) => setAvatarUrl(event.target.value)}
                placeholder="https://example.com/avatar.png"
                spellCheck={false}
                value={avatarUrl}
              />
              <p className="text-xs text-muted-foreground">
                Optional. Deployed agents fall back to the runtime avatar if
                this is blank.
              </p>
            </div>

            <div className="space-y-1.5">
              <label
                className="text-sm font-medium"
                htmlFor="persona-system-prompt"
              >
                System prompt
              </label>
              <Textarea
                className="min-h-40"
                disabled={isPending}
                id="persona-system-prompt"
                onChange={(event) => setSystemPrompt(event.target.value)}
                placeholder="Describe what this persona should do."
                value={systemPrompt}
              />
            </div>

            {enableImportDrop ? (
              <div className="flex items-center gap-3 rounded-xl border border-dashed border-border/80 bg-muted/15 px-4 py-3">
                <Upload className="h-4 w-4 shrink-0 text-muted-foreground/60" />
                <p className="text-xs text-muted-foreground">
                  Drag a .persona.json, .persona.png, or .zip onto this dialog
                  to import.
                </p>
              </div>
            ) : null}

            {error ? (
              <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {error.message}
              </p>
            ) : null}

            {importError ? (
              <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {importError}
              </p>
            ) : null}
          </div>

          <div className="flex justify-end gap-2 border-t border-border/60 px-6 py-4">
            <Button
              onClick={() => handleOpenChange(false)}
              size="sm"
              type="button"
              variant="outline"
            >
              Cancel
            </Button>
            <Button
              disabled={
                displayName.trim().length === 0 ||
                systemPrompt.trim().length === 0 ||
                isPending ||
                isParsing
              }
              onClick={() => void handleSubmit()}
              size="sm"
              type="button"
            >
              {isPending ? "Saving..." : submitLabel}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
