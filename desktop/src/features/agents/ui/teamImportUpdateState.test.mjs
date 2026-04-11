import assert from "node:assert/strict";
import test from "node:test";

import {
  getAddMemberSecondaryText,
  getMissingMemberSecondaryText,
  hasAnyImportChanges,
} from "./teamImportUpdateState.ts";

function createPlan(overrides = {}) {
  return {
    matchedMembers: [],
    membersToUpdate: [],
    newMembers: [],
    missingMembers: [],
    unresolvedPersonaIds: [],
    teamNameChanged: false,
    teamDescriptionChanged: false,
    ...overrides,
  };
}

test("hasAnyImportChanges returns false when all diffs are zero and no member changes", () => {
  assert.equal(
    hasAnyImportChanges(createPlan(), {
      addedLines: 0,
      removedLines: 0,
    }),
    false,
  );
});

test("hasAnyImportChanges returns true when team info has line changes", () => {
  assert.equal(
    hasAnyImportChanges(createPlan(), {
      addedLines: 1,
      removedLines: 0,
    }),
    true,
  );
  assert.equal(
    hasAnyImportChanges(createPlan(), {
      addedLines: 0,
      removedLines: 1,
    }),
    true,
  );
});

test("hasAnyImportChanges returns true for updated/new/missing members", () => {
  assert.equal(
    hasAnyImportChanges(
      createPlan({
        membersToUpdate: [{}],
      }),
      { addedLines: 0, removedLines: 0 },
    ),
    true,
  );
  assert.equal(
    hasAnyImportChanges(
      createPlan({
        newMembers: [{}],
      }),
      { addedLines: 0, removedLines: 0 },
    ),
    true,
  );
  assert.equal(
    hasAnyImportChanges(
      createPlan({
        missingMembers: [{}],
      }),
      { addedLines: 0, removedLines: 0 },
    ),
    true,
  );
});

test("getAddMemberSecondaryText uses explicit unselected message", () => {
  assert.equal(
    getAddMemberSecondaryText(true, "Imported prompt preview"),
    "Imported prompt preview",
  );
  assert.equal(
    getAddMemberSecondaryText(false, "Imported prompt preview"),
    "Will not be added to this team",
  );
});

test("getMissingMemberSecondaryText uses current info when unselected", () => {
  assert.equal(
    getMissingMemberSecondaryText(true, "Current prompt preview"),
    "Will be removed from this team",
  );
  assert.equal(
    getMissingMemberSecondaryText(false, "Current prompt preview"),
    "Current prompt preview",
  );
});
