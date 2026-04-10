import type { TeamImportPlan } from "./teamImportPlan";

export type LineChangeCounts = {
  addedLines: number;
  removedLines: number;
};

export function hasAnyImportChanges(
  plan: TeamImportPlan | null,
  teamLineChanges: LineChangeCounts,
): boolean {
  return (
    teamLineChanges.addedLines > 0 ||
    teamLineChanges.removedLines > 0 ||
    (plan?.membersToUpdate.length ?? 0) > 0 ||
    (plan?.newMembers.length ?? 0) > 0 ||
    (plan?.missingMembers.length ?? 0) > 0
  );
}

export function getAddMemberSecondaryText(
  shouldAdd: boolean,
  importedPromptPreview: string,
): string {
  return shouldAdd ? importedPromptPreview : "Will not be added to this team";
}

export function getMissingMemberSecondaryText(
  shouldRemove: boolean,
  currentPromptPreview: string,
): string {
  return shouldRemove ? "Will be removed from this team" : currentPromptPreview;
}
