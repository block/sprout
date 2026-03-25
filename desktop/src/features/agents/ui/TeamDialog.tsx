import * as React from "react";

import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import type {
  AgentPersona,
  CreateTeamInput,
  UpdateTeamInput,
} from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import { Checkbox } from "@/shared/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";
import { Textarea } from "@/shared/ui/textarea";

type TeamDialogProps = {
  open: boolean;
  title: string;
  description: string;
  submitLabel: string;
  initialValues: CreateTeamInput | UpdateTeamInput | null;
  personas: AgentPersona[];
  error: Error | null;
  isPending: boolean;
  onOpenChange: (open: boolean) => void;
  onSubmit: (input: CreateTeamInput | UpdateTeamInput) => Promise<void>;
};

export function TeamDialog({
  open,
  title,
  description,
  submitLabel,
  initialValues,
  personas,
  error,
  isPending,
  onOpenChange,
  onSubmit,
}: TeamDialogProps) {
  const [name, setName] = React.useState("");
  const [teamDescription, setTeamDescription] = React.useState("");
  const [selectedPersonaIds, setSelectedPersonaIds] = React.useState<string[]>(
    [],
  );

  React.useEffect(() => {
    if (!open || !initialValues) {
      return;
    }

    setName(initialValues.name);
    setTeamDescription(initialValues.description ?? "");
    setSelectedPersonaIds(initialValues.personaIds);
  }, [initialValues, open]);

  function handleOpenChange(next: boolean) {
    if (!next) {
      setName("");
      setTeamDescription("");
      setSelectedPersonaIds([]);
    }

    onOpenChange(next);
  }

  function togglePersona(personaId: string) {
    setSelectedPersonaIds((current) =>
      current.includes(personaId)
        ? current.filter((id) => id !== personaId)
        : [...current, personaId],
    );
  }

  async function handleSubmit() {
    if (!initialValues) {
      return;
    }

    const baseInput = {
      name,
      description: teamDescription.trim() || undefined,
      personaIds: selectedPersonaIds,
    };

    if ("id" in initialValues) {
      await onSubmit({ id: initialValues.id, ...baseInput });
      return;
    }

    await onSubmit(baseInput);
  }

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent className="max-w-2xl overflow-hidden p-0">
        <div className="flex max-h-[85vh] flex-col">
          <DialogHeader className="shrink-0 border-b border-border/60 px-6 py-5 pr-14">
            <DialogTitle>{title}</DialogTitle>
            <DialogDescription>{description}</DialogDescription>
          </DialogHeader>

          <div className="min-h-0 flex-1 space-y-5 overflow-y-auto px-6 py-5">
            <div className="space-y-1.5">
              <label className="text-sm font-medium" htmlFor="team-name">
                Name
              </label>
              <Input
                autoCorrect="off"
                disabled={isPending}
                id="team-name"
                onChange={(event) => setName(event.target.value)}
                placeholder="Engineering Squad"
                value={name}
              />
            </div>

            <div className="space-y-1.5">
              <label className="text-sm font-medium" htmlFor="team-description">
                Description
              </label>
              <Textarea
                className="min-h-20"
                disabled={isPending}
                id="team-description"
                onChange={(event) => setTeamDescription(event.target.value)}
                placeholder="Optional description for this team."
                value={teamDescription}
              />
            </div>

            <div className="space-y-2">
              <span className="text-sm font-medium">Personas</span>
              <p className="text-xs text-muted-foreground">
                Select the personas to include in this team.
              </p>
              {personas.length === 0 ? (
                <p className="py-4 text-center text-sm text-muted-foreground">
                  No personas available. Create one first.
                </p>
              ) : (
                <div
                  className="max-h-60 space-y-1 overflow-y-auto rounded-lg border border-border/70 p-2"
                  role="listbox"
                  aria-label="Personas"
                  aria-multiselectable="true"
                >
                  {personas.map((persona) => {
                    const isSelected = selectedPersonaIds.includes(persona.id);

                    return (
                      <div
                        className="flex cursor-pointer items-center gap-3 rounded-md px-2 py-1.5 transition-colors hover:bg-muted/50"
                        key={persona.id}
                        onClick={() => {
                          if (!isPending) {
                            togglePersona(persona.id);
                          }
                        }}
                        onKeyDown={(event) => {
                          if (
                            !isPending &&
                            (event.key === "Enter" || event.key === " ")
                          ) {
                            event.preventDefault();
                            togglePersona(persona.id);
                          }
                        }}
                        role="option"
                        aria-selected={isSelected}
                        tabIndex={0}
                      >
                        <Checkbox
                          checked={isSelected}
                          disabled={isPending}
                          onCheckedChange={() => togglePersona(persona.id)}
                        />
                        <ProfileAvatar
                          avatarUrl={persona.avatarUrl}
                          className="h-6 w-6 rounded-full text-[10px]"
                          label={persona.displayName}
                        />
                        <span className="text-sm">{persona.displayName}</span>
                        {persona.isBuiltIn ? (
                          <span className="rounded-full bg-muted px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
                            Built-in
                          </span>
                        ) : null}
                      </div>
                    );
                  })}
                </div>
              )}
            </div>

            {error ? (
              <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {error.message}
              </p>
            ) : null}
          </div>

          <div className="flex shrink-0 justify-end gap-2 border-t border-border/60 px-6 py-4">
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
                name.trim().length === 0 ||
                selectedPersonaIds.length === 0 ||
                isPending
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
