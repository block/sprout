import assert from "node:assert/strict";
import test from "node:test";

import {
  getFieldPreview,
  getFieldSecondaryText,
  getSelectedFieldCount,
} from "./personaImportUpdateState.ts";

test("getFieldSecondaryText returns imported preview when selected", () => {
  assert.equal(
    getFieldSecondaryText(true, "New prompt text", "Old prompt text"),
    "New prompt text",
  );
});

test("getFieldSecondaryText returns current value when not selected", () => {
  assert.equal(
    getFieldSecondaryText(false, "New prompt text", "Old prompt text"),
    "Old prompt text",
  );
});

test("getFieldPreview returns fallback for empty values", () => {
  assert.equal(getFieldPreview("", "No value."), "No value.");
  assert.equal(getFieldPreview("   ", "No value."), "No value.");
  assert.equal(getFieldPreview("\n\n", "No value."), "No value.");
});

test("getFieldPreview returns first non-empty line", () => {
  assert.equal(getFieldPreview("Hello world", "fallback"), "Hello world");
  assert.equal(
    getFieldPreview("\n  First line\nSecond line", "fallback"),
    "First line",
  );
});

test("getFieldPreview truncates long lines", () => {
  const longLine = "a".repeat(300);
  const result = getFieldPreview(longLine, "fallback", 240);
  assert.equal(result.length, 241);
  assert.ok(result.endsWith("…"));
});

test("getSelectedFieldCount counts selected fields", () => {
  const fields = [
    {
      field: "displayName",
      label: "Display name",
      existingValue: "A",
      importedValue: "B",
      addedLines: 1,
      removedLines: 1,
    },
    {
      field: "systemPrompt",
      label: "System prompt",
      existingValue: "X",
      importedValue: "Y",
      addedLines: 1,
      removedLines: 1,
    },
    {
      field: "model",
      label: "Preferred model",
      existingValue: "m1",
      importedValue: "m2",
      addedLines: 1,
      removedLines: 1,
    },
  ];

  assert.equal(
    getSelectedFieldCount(fields, new Set(["displayName", "model"])),
    2,
  );
  assert.equal(getSelectedFieldCount(fields, new Set()), 0);
  assert.equal(
    getSelectedFieldCount(
      fields,
      new Set(["displayName", "systemPrompt", "model"]),
    ),
    3,
  );
});
