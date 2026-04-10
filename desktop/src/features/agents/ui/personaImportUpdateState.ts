import type { PersonaImportFieldChange } from "./personaImportPlan";

export function getFieldSecondaryText(
  shouldUpdate: boolean,
  importedPreview: string,
  currentPreview: string,
): string {
  return shouldUpdate ? importedPreview : currentPreview;
}

export function getFieldPreview(
  value: string,
  emptyFallback: string,
  maxLength = 240,
): string {
  const normalized = value.replace(/\r\n/g, "\n").trim();
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

export function getSelectedFieldCount(
  fields: PersonaImportFieldChange[],
  selectedFields: Set<string>,
): number {
  return fields.filter((field) => selectedFields.has(field.field)).length;
}
