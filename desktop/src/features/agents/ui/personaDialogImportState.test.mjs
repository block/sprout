import assert from "node:assert/strict";
import test from "node:test";

import {
  getImportButtonLabel,
  getImportButtonTone,
  getImportErrorLabel,
  IMPORT_ERROR_VISIBILITY_MS,
} from "./personaDialogImportState.ts";

test("getImportErrorLabel falls back to Invalid format for empty values", () => {
  assert.equal(getImportErrorLabel(null), "Invalid format");
  assert.equal(getImportErrorLabel(""), "Invalid format");
  assert.equal(getImportErrorLabel("   "), "Invalid format");
});

test("getImportErrorLabel keeps meaningful parse errors", () => {
  assert.equal(
    getImportErrorLabel("Failed to parse persona file."),
    "Failed to parse persona file.",
  );
});

test("import button label prioritizes drag-drop affordance", () => {
  assert.equal(
    getImportButtonLabel({
      isWindowFileDragOver: true,
      isImportDragOver: false,
      importErrorMessage: "Invalid format",
    }),
    "Drop .persona.json to import",
  );
  assert.equal(
    getImportButtonLabel({
      isWindowFileDragOver: false,
      isImportDragOver: true,
      importErrorMessage: "Invalid format",
    }),
    "Drop .persona.json to import",
  );
});

test("import button label shows error when not dragging", () => {
  assert.equal(
    getImportButtonLabel({
      isWindowFileDragOver: false,
      isImportDragOver: false,
      importErrorMessage: "Invalid format",
    }),
    "Invalid format",
  );
  assert.equal(
    getImportButtonLabel({
      isWindowFileDragOver: false,
      isImportDragOver: false,
      importErrorMessage: null,
    }),
    "Import",
  );
});

test("import button tone follows drag > error > default priority", () => {
  assert.equal(
    getImportButtonTone({
      isWindowFileDragOver: true,
      isImportDragOver: false,
      importErrorMessage: "Invalid format",
    }),
    "drag",
  );
  assert.equal(
    getImportButtonTone({
      isWindowFileDragOver: false,
      isImportDragOver: false,
      importErrorMessage: "Invalid format",
    }),
    "error",
  );
  assert.equal(
    getImportButtonTone({
      isWindowFileDragOver: false,
      isImportDragOver: false,
      importErrorMessage: null,
    }),
    "default",
  );
});

test("import error visibility duration is 5 seconds", () => {
  assert.equal(IMPORT_ERROR_VISIBILITY_MS, 5_000);
});
