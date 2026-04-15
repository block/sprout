import assert from "node:assert/strict";
import test from "node:test";

import {
  createPersonaDialogState,
  duplicatePersonaDialogState,
  editPersonaDialogState,
  importPersonaDialogState,
} from "./personaDialogState.ts";

test("createPersonaDialogState returns a fresh empty draft", () => {
  const first = createPersonaDialogState();
  const second = createPersonaDialogState();

  assert.equal(first.title, "Create persona");
  assert.deepEqual(first.initialValues, {
    displayName: "",
    avatarUrl: "",
    systemPrompt: "",
    provider: undefined,
    model: undefined,
  });
  assert.notStrictEqual(first.initialValues, second.initialValues);
});

test("duplicatePersonaDialogState copies persona fields into a new draft", () => {
  const state = duplicatePersonaDialogState({
    id: "persona-1",
    displayName: "Solo",
    avatarUrl: "avatar://solo",
    systemPrompt: "Be direct.",
    provider: "provider-a",
    model: "model-a",
    isBuiltIn: false,
    isActive: true,
    createdAt: "2025-01-01T00:00:00Z",
    updatedAt: "2025-01-02T00:00:00Z",
  });

  assert.deepEqual(state.initialValues, {
    displayName: "Solo copy",
    avatarUrl: "avatar://solo",
    systemPrompt: "Be direct.",
    provider: "provider-a",
    model: "model-a",
  });
});

test("editPersonaDialogState preserves the persona id for updates", () => {
  const state = editPersonaDialogState({
    id: "persona-2",
    displayName: "Ralph",
    avatarUrl: null,
    systemPrompt: "Keep it weird.",
    provider: null,
    model: null,
    isBuiltIn: true,
    isActive: true,
    createdAt: "2025-01-01T00:00:00Z",
    updatedAt: "2025-01-02T00:00:00Z",
  });

  assert.equal(state.title, "Edit persona");
  assert.equal(state.description, "");
  assert.equal(state.submitLabel, "Save changes");
  assert.deepEqual(state.initialValues, {
    id: "persona-2",
    displayName: "Ralph",
    avatarUrl: "",
    systemPrompt: "Keep it weird.",
    provider: undefined,
    model: undefined,
  });
});

test("importPersonaDialogState maps parsed persona previews into create drafts", () => {
  const state = importPersonaDialogState({
    displayName: "Imported",
    avatarDataUrl: null,
    systemPrompt: "Imported prompt",
    provider: null,
    model: "model-b",
    sourceFile: "import.persona.json",
  });

  assert.equal(state.title, "Import Imported");
  assert.deepEqual(state.initialValues, {
    displayName: "Imported",
    avatarUrl: "",
    systemPrompt: "Imported prompt",
    provider: undefined,
    model: "model-b",
  });
});
