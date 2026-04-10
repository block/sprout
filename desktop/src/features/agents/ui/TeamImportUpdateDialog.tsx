import * as React from "react";
import { Users } from "lucide-react";

import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/shared/ui/alert-dialog";
import { Button } from "@/shared/ui/button";
import { Checkbox } from "@/shared/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import type { ParsedTeamPreview } from "@/shared/api/tauriTeams";
import type { AgentPersona, AgentTeam } from "@/shared/api/types";
import { buildTeamImportPlan } from "./teamImportPlan";

type TeamImportUpdateDialogProps = {
  open: boolean;
  team: AgentTeam | null;
  personas: AgentPersona[];
  preview: ParsedTeamPreview | null;
  fileName: string;
  isPending: boolean;
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

  function renderLineChangeSummary(addedLines: number, removedLines: number) {
    return (
      <p className="shrink-0 text-xs font-medium tabular-nums">
        <span
          className={
            addedLines > 0 ? "text-status-added" : "text-muted-foreground"
          }
        >
          +{addedLines}
        </span>
        <span className="text-muted-foreground"> / </span>
        <span
          className={
            removedLines > 0 ? "text-status-deleted" : "text-muted-foreground"
          }
        >
          -{removedLines}
        </span>
      </p>
    );
  }

  return (
    <>
      <Dialog onOpenChange={onOpenChange} open={open}>
        <DialogContent className="flex max-h-[85vh] max-w-3xl flex-col overflow-hidden p-0">
          <DialogHeader className="shrink-0 border-b border-border/60 px-6 py-5 pr-14">
            <DialogTitle>Import team</DialogTitle>
            <DialogDescription>
              Review this import before applying updates to team info and
              members.
            </DialogDescription>
          </DialogHeader>

          <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
            <div className="space-y-4">
              <div className="flex items-center gap-3 rounded-lg border border-border/60 bg-card/80 px-4 py-3">
                <Users className="h-5 w-5 shrink-0 text-muted-foreground" />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-semibold tracking-tight">
                    {team?.name ?? "Selected team"}
                  </p>
                  {team?.description ? (
                    <p className="text-xs text-muted-foreground">
                      {team.description}
                    </p>
                  ) : (
                    <p className="text-xs text-muted-foreground">
                      No team description
                    </p>
                  )}
                </div>
                <span className="text-xs text-muted-foreground">
                  {team?.personaIds.length ?? 0} member
                  {(team?.personaIds.length ?? 0) === 1 ? "" : "s"}
                </span>
              </div>

              {fileName ? (
                <div className="rounded-lg border border-border/60 bg-card/80 px-4 py-3">
                  <p className="text-xs text-muted-foreground">Import file</p>
                  <p className="truncate text-sm font-medium">{fileName}</p>
                </div>
              ) : null}

              {preview && plan ? (
                <div className="space-y-4">
                  <div className="rounded-lg border border-border/60 bg-card/80 px-4 py-3">
                    <div className="flex items-start gap-2.5">
                      <Checkbox
                        checked={updateTeamInfo}
                        disabled={isPending}
                        onCheckedChange={(checked) =>
                          setUpdateTeamInfo(Boolean(checked))
                        }
                      />
                      <div className="space-y-1">
                        <p className="text-sm font-medium">
                          Update team info from import
                        </p>
                        <p className="text-xs text-muted-foreground">
                          {plan.teamNameChanged || plan.teamDescriptionChanged
                            ? `Will change to "${preview.name}"${preview.description ? ` — ${preview.description}` : ""}.`
                            : "Imported team info matches current values."}
                        </p>
                      </div>
                    </div>
                  </div>

                  <div className="space-y-1">
                    <p className="text-sm font-medium">
                      Members that will be updated ({selectedUpdatedCount}/
                      {plan.membersToUpdate.length})
                    </p>
                    {plan.membersToUpdate.length > 0 ? (
                      <div className="space-y-1">
                        {plan.membersToUpdate.map((member) => (
                          <div
                            className="flex items-center gap-3 rounded-lg border border-border/60 bg-card/80 px-3 py-2.5"
                            key={member.existing.id}
                          >
                            <Checkbox
                              checked={selectedUpdatedPersonaIds.has(
                                member.existing.id,
                              )}
                              disabled={isPending}
                              onCheckedChange={(checked) =>
                                toggleUpdatedPersona(
                                  member.existing.id,
                                  Boolean(checked),
                                )
                              }
                            />
                            <ProfileAvatar
                              avatarUrl={
                                member.imported.avatar_url ??
                                member.existing.avatarUrl
                              }
                              className="h-8 w-8 rounded-lg text-xs"
                              label={member.imported.display_name}
                            />
                            <div className="min-w-0 flex-1">
                              <p className="truncate text-sm font-semibold tracking-tight">
                                {member.existing.displayName}
                              </p>
                              <p className="truncate text-xs text-muted-foreground">
                                Updated from import data
                              </p>
                            </div>
                            {renderLineChangeSummary(
                              member.addedLines,
                              member.removedLines,
                            )}
                          </div>
                        ))}
                      </div>
                    ) : (
                      <p className="text-xs text-muted-foreground">
                        No existing members need updates.
                      </p>
                    )}
                  </div>

                  <div className="space-y-1">
                    <p className="text-sm font-medium">
                      New members to add ({selectedNewCount}/
                      {plan.newMembers.length})
                    </p>
                    {plan.newMembers.length > 0 ? (
                      <div className="space-y-1">
                        {plan.newMembers.map((member) => (
                          <div
                            className="flex items-center gap-3 rounded-lg border border-border/60 bg-card/80 px-3 py-2.5"
                            key={`${member.importedIndex}-${member.imported.display_name}`}
                          >
                            <Checkbox
                              checked={selectedNewMemberIndexes.has(
                                member.importedIndex,
                              )}
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
                              <p className="truncate text-xs text-muted-foreground">
                                Will be created from import
                              </p>
                            </div>
                            {renderLineChangeSummary(member.addedLines, 0)}
                          </div>
                        ))}
                      </div>
                    ) : (
                      <p className="text-xs text-muted-foreground">
                        No new members in this import.
                      </p>
                    )}
                  </div>

                  <div className="space-y-1">
                    <p className="text-sm font-medium">
                      Current members not in import (
                      {plan.missingMembers.length})
                    </p>
                    {plan.missingMembers.length > 0 ? (
                      <div className="space-y-1">
                        {plan.missingMembers.map((member) => {
                          const shouldRemove = missingPersonaIdsToRemove.has(
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
                                <p className="truncate text-xs text-muted-foreground">
                                  {shouldRemove
                                    ? "Will be removed from this team"
                                    : "Leave unedited in this team"}
                                </p>
                              </div>
                              {renderLineChangeSummary(0, member.removedLines)}
                            </div>
                          );
                        })}
                      </div>
                    ) : (
                      <p className="text-xs text-muted-foreground">
                        All current members are represented in the import.
                      </p>
                    )}
                  </div>

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

      <AlertDialog
        onOpenChange={setConfirmRemovalOpen}
        open={confirmRemovalOpen}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              Remove {removableCount} member{removableCount === 1 ? "" : "s"}?
            </AlertDialogTitle>
            <AlertDialogDescription>
              These members will be removed from this team:{" "}
              {removableMembers
                .map((member) => member.existing.displayName)
                .join(", ")}
              . Do you also want to remove those agents from My Agents?
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <Button
              onClick={() => setConfirmRemovalOpen(false)}
              size="sm"
              type="button"
              variant="outline"
            >
              Go back
            </Button>
            <Button
              disabled={isPending}
              onClick={() => {
                void runApply(false);
              }}
              size="sm"
              type="button"
              variant="outline"
            >
              Keep agents
            </Button>
            <Button
              disabled={isPending}
              onClick={() => {
                void runApply(true);
              }}
              size="sm"
              type="button"
              variant="destructive"
            >
              Remove agents too
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
