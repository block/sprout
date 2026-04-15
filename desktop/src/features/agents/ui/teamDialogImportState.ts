type ImportButtonStateInput = {
  isWindowFileDragOver: boolean;
  isImportDragOver: boolean;
  importErrorMessage: string | null;
};

export const IMPORT_ERROR_VISIBILITY_MS = 5_000;

export function getImportErrorLabel(errorMessage: string | null): string {
  const message = (errorMessage ?? "").trim();
  return message.length > 0 ? message : "Invalid format";
}

export function getImportButtonLabel({
  isWindowFileDragOver,
  isImportDragOver,
  importErrorMessage,
}: ImportButtonStateInput): string {
  if (isWindowFileDragOver || isImportDragOver) {
    return "Drop .team.json to import";
  }
  if (importErrorMessage !== null) {
    return getImportErrorLabel(importErrorMessage);
  }
  return "Import";
}

export function getImportButtonTone({
  isWindowFileDragOver,
  isImportDragOver,
  importErrorMessage,
}: ImportButtonStateInput): "drag" | "error" | "default" {
  if (isWindowFileDragOver || isImportDragOver) {
    return "drag";
  }
  if (importErrorMessage !== null) {
    return "error";
  }
  return "default";
}
