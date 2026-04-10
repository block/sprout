import * as React from "react";
import { Users } from "lucide-react";

import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import { Button } from "@/shared/ui/button";
import { Checkbox } from "@/shared/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import type { ParsedTeamPreview } from "@/shared/api/tauriTeams";
import type { AgentPersona, AgentTeam } from "@/shared/api/types";
import { RemoveMembersConfirmDialog } from "./RemoveMembersConfirmDialog";
import { buildTeamImportPlan } from "./teamImportPlan";
import {
  getAddMemberSecondaryText,
  getMissingMemberSecondaryText,
  hasAnyImportChanges,
} from "./teamImportUpdateState";

type TeamImportUpdateDialogProps = {
  open: boolean;
  team: AgentTeam | null;
  personas: AgentPersona[];
  preview: ParsedTeamPreview | null;
  fileName: string;
  isPending: boolean;
  onClear: () => void;
  onOpenChange: (open: boolean) => void;
  onApply: (input: {
    personas: AgentPersona[];
    updateTeamInfo: boolean;
    selectedUpdatedPersonaIds: string[];
    selectedNewMemberIndexes: number[];
    missingPersonaIdsToRemove: string[];
    deleteRemovedAgents: boolean;
  }) => Promise<void>;
};

export function TeamImportUpdateDialog({
  open,
  team,
  personas,
  preview,
  fileName,
  isPending,
  onClear,
  onOpenChange,
  onApply,
}: TeamImportUpdateDialogProps) {
  const [updateTeamInfo, setUpdateTeamInfo] = React.useState(true);
  const [selectedUpdatedPersonaIds, setSelectedUpdatedPersonaIds] =
    React.useState<Set<string>>(new Set());
  const [selectedNewMemberIndexes, setSelectedNewMemberIndexes] =
    React.useState<Set<number>>(new Set());
  const [missingPersonaIdsToRemove, setMissingPersonaIdsToRemove] =
    React.useState<Set<string>>(new Set());
  const [confirmRemovalOpen, setConfirmRemovalOpen] = React.useState(false);
  const [errorMessage, setErrorMessage] = React.useState<string | null>(null);

  const plan = React.useMemo(() => {
    if (!team || !preview) {
      return null;
    }
    return buildTeamImportPlan({ team, personas, preview });
  }, [team, personas, preview]);

  React.useEffect(() => {
    if (!open) {
      return;
    }
    setErrorMessage(null);
    setUpdateTeamInfo(true);
    setSelectedUpdatedPersonaIds(new Set());
    setSelectedNewMemberIndexes(new Set());
    setMissingPersonaIdsToRemove(new Set());
    setConfirmRemovalOpen(false);
  }, [open]);

  React.useEffect(() => {
    if (!open || !plan) {
      return;
    }

    setSelectedUpdatedPersonaIds(
      new Set(plan.membersToUpdate.map((member) => member.existing.id)),
    );
    setSelectedNewMemberIndexes(
      new Set(plan.newMembers.map((member) => member.importedIndex)),
    );
  }, [open, plan]);

  function toggleMissingPersona(personaId: string, checked: boolean) {
    setMissingPersonaIdsToRemove((current) => {
      const next = new Set(current);
      if (checked) {
        next.add(personaId);
      } else {
        next.delete(personaId);
      }
      return next;
    });
  }

  function toggleUpdatedPersona(personaId: string, checked: boolean) {
    setSelectedUpdatedPersonaIds((current) => {
      const next = new Set(current);
      if (checked) {
        next.add(personaId);
      } else {
        next.delete(personaId);
      }
      return next;
    });
  }

  function toggleNewMember(importedIndex: number, checked: boolean) {
    setSelectedNewMemberIndexes((current) => {
      const next = new Set(current);
      if (checked) {
        next.add(importedIndex);
      } else {
        next.delete(importedIndex);
      }
      return next;
    });
  }

  async function runApply(deleteRemovedAgents: boolean) {
    setErrorMessage(null);
    try {
      await onApply({
        personas,
        updateTeamInfo,
        selectedUpdatedPersonaIds: Array.from(selectedUpdatedPersonaIds),
        selectedNewMemberIndexes: Array.from(selectedNewMemberIndexes),
        missingPersonaIdsToRemove: Array.from(missingPersonaIdsToRemove),
        deleteRemovedAgents,
      });
      setConfirmRemovalOpen(false);
    } catch (error) {
      setErrorMessage(
        error instanceof Error
          ? error.message
          : "Failed to apply imported team update.",
      );
    }
  }

  const removableCount = missingPersonaIdsToRemove.size;
  const removableMembers =
    plan?.missingMembers.filter((member) =>
      missingPersonaIdsToRemove.has(member.existing.id),
    ) ?? [];
  const selectedUpdatedCount = selectedUpdatedPersonaIds.size;
  const selectedNewCount = selectedNewMemberIndexes.size;

  function renderLineChangeSummary(
    addedLines: number,
    removedLines: number,
    emphasize = true,
  ) {
    const addedClass = emphasize
      ? addedLines > 0
        ? "text-status-added"
        : "text-muted-foreground"
      : "text-muted-foreground";
    const separatorClass = emphasize
      ? "text-muted-foreground"
      : "text-muted-foreground";
    const removedClass = emphasize
      ? removedLines > 0
        ? "text-status-deleted"
        : "text-muted-foreground"
      : "text-muted-foreground";
    const opacityClass = emphasize ? "opacity-100" : "opacity-50";

    return (
      <p
        className={`shrink-0 text-xs font-medium tabular-nums transition-opacity ${opacityClass}`}
      >
        <span className={addedClass}>+{addedLines}</span>
        <span className={separatorClass}> / </span>
        <span className={removedClass}>-{removedLines}</span>
      </p>
    );
  }

  function getPromptPreview(
    prompt: string,
    emptyFallback: string,
    maxLength = 240,
  ): string {
    const normalized = prompt.replace(/\r\n/g, "\n").trim();
    if (normalized.length === 0) {
      return emptyFallback;
    }
    const firstNonEmptyLine =
      normalized
        .split("\n")
        .map((line) => line.trim())
        .find((line) => line.length > 0) ?? normalized;
    if (firstNonEmptyLine.length <= maxLength) {
      return firstNonEmptyLine;
    }
    return `${firstNonEmptyLine.slice(0, maxLength).trimEnd()}…`;
  }

  function getTeamNamePreview(teamName: string | null | undefined): string {
    return (teamName ?? "").trim() || "Untitled team";
  }

  function normalizeTeamTextLines(value: string): string[] {
    const normalized = value.replace(/\r\n/g, "\n");
    const lines = normalized.split("\n").map((line) => line.trimEnd());
    while (lines.length > 0 && lines[lines.length - 1] === "") {
      lines.pop();
    }
    return lines;
  }

  function getTeamSnapshotLines(
    teamName: string | null | undefined,
    description: string | null | undefined,
  ): string[] {
    return [
      `name:${(teamName ?? "").trim()}`,
      ...normalizeTeamTextLines(description ?? "").map(
        (line) => `description:${line}`,
      ),
    ];
  }

  function countLineChanges(
    previousLines: string[],
    nextLines: string[],
  ): {
    addedLines: number;
    removedLines: number;
  } {
    const previousLength = previousLines.length;
    const nextLength = nextLines.length;

    if (previousLength === 0) {
      return { addedLines: nextLength, removedLines: 0 };
    }
    if (nextLength === 0) {
      return { addedLines: 0, removedLines: previousLength };
    }

    const lcs = Array.from({ length: previousLength + 1 }, () =>
      Array<number>(nextLength + 1).fill(0),
    );

    for (let i = previousLength - 1; i >= 0; i -= 1) {
      for (let j = nextLength - 1; j >= 0; j -= 1) {
        if (previousLines[i] === nextLines[j]) {
          lcs[i][j] = lcs[i + 1][j + 1] + 1;
        } else {
          lcs[i][j] = Math.max(lcs[i + 1][j], lcs[i][j + 1]);
        }
      }
    }

    let i = 0;
    let j = 0;
    let addedLines = 0;
    let removedLines = 0;

    while (i < previousLength && j < nextLength) {
      if (previousLines[i] === nextLines[j]) {
        i += 1;
        j += 1;
        continue;
      }
      if (lcs[i + 1][j] >= lcs[i][j + 1]) {
        removedLines += 1;
        i += 1;
      } else {
        addedLines += 1;
        j += 1;
      }
    }

    removedLines += previousLength - i;
    addedLines += nextLength - j;

    return { addedLines, removedLines };
  }

  const teamLineChanges =
    team && preview
      ? countLineChanges(
          getTeamSnapshotLines(team.name, team.description),
          getTeamSnapshotLines(preview.name, preview.description),
        )
      : { addedLines: 0, removedLines: 0 };
  const hasChanges = hasAnyImportChanges(plan, teamLineChanges);

  return (
    <>
      <Dialog onOpenChange={onOpenChange} open={open}>
        <DialogContent className="flex max-h-[85vh] max-w-3xl flex-col overflow-hidden p-0">
          <DialogHeader className="shrink-0 space-y-1 border-b border-border/60 px-6 py-5 pr-14">
            <DialogTitle>Import team</DialogTitle>
          </DialogHeader>

          <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
            <div className="space-y-4">
              <div className="flex items-center justify-between gap-3 rounded-md border border-border/60 bg-card/80 px-3 py-2">
                <p className="min-w-0 truncate text-sm font-medium">
                  {fileName || "Imported file"}
                </p>
                <button
                  aria-label="Clear import"
                  className="shrink-0 text-sm text-primary underline-offset-4 hover:underline"
                  disabled={isPending}
                  onClick={onClear}
                  type="button"
                >
                  Clear
                </button>
              </div>

              {preview && plan ? (
                <div className="space-y-4">
                  {hasChanges ? (
                    <>
                      <div className="space-y-1">
                        <p className="text-sm font-medium">Team info</p>
                        <div className="flex items-center gap-3 rounded-lg border border-border/60 bg-card/80 px-3 py-2.5">
                          <Checkbox
                            checked={updateTeamInfo}
                            disabled={isPending}
                            onCheckedChange={(checked) =>
                              setUpdateTeamInfo(Boolean(checked))
                            }
                          />
                          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground">
                            <Users className="h-4 w-4" />
                          </div>
                          <div className="min-w-0 flex-1">
                            <p className="truncate text-sm font-semibold tracking-tight">
                              {updateTeamInfo
                                ? getTeamNamePreview(preview.name)
                                : getTeamNamePreview(team?.name)}
                            </p>
                            <p
                              className={`truncate text-xs ${
                                updateTeamInfo
                                  ? "text-foreground"
                                  : "text-muted-foreground"
                              }`}
                            >
                              {updateTeamInfo
                                ? getPromptPreview(
                                    preview.description ?? "",
                                    "No team description in import.",
                                  )
                                : getPromptPreview(
                                    team?.description ?? "",
                                    "No current team description.",
                                  )}
                            </p>
                          </div>
                          {renderLineChangeSummary(
                            teamLineChanges.addedLines,
                            teamLineChanges.removedLines,
                            updateTeamInfo,
                          )}
                        </div>
                      </div>

                      <div className="space-y-1">
                        <p className="text-sm font-medium">
                          Members that will be updated{" "}
                          <span className="font-bold">
                            ({selectedUpdatedCount}/
                            {plan.membersToUpdate.length})
                          </span>
                        </p>
                        {plan.membersToUpdate.length > 0 ? (
                          <div className="space-y-1">
                            {plan.membersToUpdate.map((member) => {
                              const shouldUpdate =
                                selectedUpdatedPersonaIds.has(
                                  member.existing.id,
                                );
                              const previewAvatarUrl = shouldUpdate
                                ? (member.imported.avatar_url ??
                                  member.existing.avatarUrl)
                                : member.existing.avatarUrl;
                              const previewName = shouldUpdate
                                ? member.imported.display_name
                                : member.existing.displayName;
                              const previewPrompt = shouldUpdate
                                ? getPromptPreview(
                                    member.imported.system_prompt,
                                    "No member text in import.",
                                  )
                                : getPromptPreview(
                                    member.existing.systemPrompt,
                                    "No current member text.",
                                  );

                              return (
                                <div
                                  className="flex items-center gap-3 rounded-lg border border-border/60 bg-card/80 px-3 py-2.5"
                                  key={member.existing.id}
                                >
                                  <Checkbox
                                    checked={shouldUpdate}
                                    disabled={isPending}
                                    onCheckedChange={(checked) =>
                                      toggleUpdatedPersona(
                                        member.existing.id,
                                        Boolean(checked),
                                      )
                                    }
                                  />
                                  <ProfileAvatar
                                    avatarUrl={previewAvatarUrl}
                                    className="h-8 w-8 rounded-lg text-xs"
                                    label={previewName}
                                  />
                                  <div className="min-w-0 flex-1">
                                    <p className="truncate text-sm font-semibold tracking-tight">
                                      {previewName}
                                    </p>
                                    <p
                                      className={`truncate text-xs ${
                                        shouldUpdate
                                          ? "text-foreground"
                                          : "text-muted-foreground"
                                      }`}
                                    >
                                      {previewPrompt}
                                    </p>
                                  </div>
                                  {renderLineChangeSummary(
                                    member.addedLines,
                                    member.removedLines,
                                    shouldUpdate,
                                  )}
                                </div>
                              );
                            })}
                          </div>
                        ) : null}
                      </div>

                      <div className="space-y-1">
                        <p className="text-sm font-medium">
                          Add members{" "}
                          <span className="font-bold">
                            ({selectedNewCount}/{plan.newMembers.length})
                          </span>
                        </p>
                        {plan.newMembers.length > 0 ? (
                          <div className="space-y-1">
                            {plan.newMembers.map((member) => {
                              const shouldAdd = selectedNewMemberIndexes.has(
                                member.importedIndex,
                              );
                              return (
                                <div
                                  className="flex items-center gap-3 rounded-lg border border-border/60 bg-card/80 px-3 py-2.5"
                                  key={`${member.importedIndex}-${member.imported.display_name}`}
                                >
                                  <Checkbox
                                    checked={shouldAdd}
                                    disabled={isPending}
                                    onCheckedChange={(checked) =>
                                      toggleNewMember(
                                        member.importedIndex,
                                        Boolean(checked),
                                      )
                                    }
                                  />
                                  <ProfileAvatar
                                    avatarUrl={member.imported.avatar_url}
                                    className="h-8 w-8 rounded-lg text-xs"
                                    label={member.imported.display_name}
                                  />
                                  <div className="min-w-0 flex-1">
                                    <p className="truncate text-sm font-semibold tracking-tight">
                                      {member.imported.display_name}
                                    </p>
                                    <p
                                      className={`truncate text-xs ${
                                        shouldAdd
                                          ? "text-foreground"
                                          : "text-muted-foreground"
                                      }`}
                                    >
                                      {getAddMemberSecondaryText(
                                        shouldAdd,
                                        getPromptPreview(
                                          member.imported.system_prompt,
                                          "No member text in import.",
                                        ),
                                      )}
                                    </p>
                                  </div>
                                  {renderLineChangeSummary(
                                    member.addedLines,
                                    0,
                                    shouldAdd,
                                  )}
                                </div>
                              );
                            })}
                          </div>
                        ) : null}
                      </div>

                      <div className="space-y-1">
                        <p className="text-sm font-medium">
                          Remove members not in import{" "}
                          <span className="font-bold">
                            ({removableCount}/{plan.missingMembers.length})
                          </span>
                        </p>
                        {plan.missingMembers.length > 0 ? (
                          <div className="space-y-1">
                            {plan.missingMembers.map((member) => {
                              const shouldRemove =
                                missingPersonaIdsToRemove.has(
                                  member.existing.id,
                                );
                              return (
                                <div
                                  className="flex items-center gap-3 rounded-lg border border-border/60 bg-card/80 px-3 py-2.5"
                                  key={member.existing.id}
                                >
                                  <Checkbox
                                    checked={shouldRemove}
                                    disabled={isPending}
                                    onCheckedChange={(checked) =>
                                      toggleMissingPersona(
                                        member.existing.id,
                                        Boolean(checked),
                                      )
                                    }
                                  />
                                  <ProfileAvatar
                                    avatarUrl={member.existing.avatarUrl}
                                    className="h-8 w-8 rounded-lg text-xs"
                                    label={member.existing.displayName}
                                  />
                                  <div className="min-w-0 flex-1">
                                    <p className="truncate text-sm font-semibold tracking-tight">
                                      {member.existing.displayName}
                                    </p>
                                    <p
                                      className={`truncate text-xs ${
                                        shouldRemove
                                          ? "text-foreground"
                                          : "text-muted-foreground"
                                      }`}
                                    >
                                      {getMissingMemberSecondaryText(
                                        shouldRemove,
                                        getPromptPreview(
                                          member.existing.systemPrompt,
                                          "No current member text.",
                                        ),
                                      )}
                                    </p>
                                  </div>
                                  {renderLineChangeSummary(
                                    0,
                                    member.removedLines,
                                    shouldRemove,
                                  )}
                                </div>
                              );
                            })}
                          </div>
                        ) : null}
                      </div>
                    </>
                  ) : (
                    <div className="rounded-lg border border-dashed border-border/60 bg-card/60 px-4 py-10 text-center">
                      <p className="text-sm font-semibold tracking-tight text-muted-foreground">
                        no changes
                      </p>
                    </div>
                  )}

                  {plan.unresolvedPersonaIds.length > 0 ? (
                    <p className="rounded-lg border border-amber-300/40 bg-amber-500/10 px-3 py-2 text-xs text-amber-800">
                      This team currently references{" "}
                      {plan.unresolvedPersonaIds.length} missing member
                      {plan.unresolvedPersonaIds.length === 1 ? "" : "s"} and
                      they cannot be updated by import.
                    </p>
                  ) : null}
                </div>
              ) : (
                <p className="text-sm text-muted-foreground">
                  No import preview is available.
                </p>
              )}

              {errorMessage ? (
                <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                  {errorMessage}
                </p>
              ) : null}
            </div>
          </div>

          <div className="flex shrink-0 justify-end gap-2 border-t border-border/60 px-6 py-4">
            <Button
              onClick={() => onOpenChange(false)}
              size="sm"
              type="button"
              variant="outline"
            >
              Cancel
            </Button>
            <Button
              disabled={!preview || isPending}
              onClick={() => {
                if (removableCount > 0) {
                  setConfirmRemovalOpen(true);
                  return;
                }
                void runApply(false);
              }}
              size="sm"
              type="button"
            >
              Apply update
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      <RemoveMembersConfirmDialog
        open={confirmRemovalOpen}
        onOpenChange={setConfirmRemovalOpen}
        isPending={isPending}
        memberNames={removableMembers.map(
          (member) => member.existing.displayName,
        )}
        onKeepAgents={() => void runApply(false)}
        onRemoveAgents={() => void runApply(true)}
      />
    </>
  );
}
