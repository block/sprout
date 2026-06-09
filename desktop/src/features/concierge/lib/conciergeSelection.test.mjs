import assert from "node:assert/strict";
import test from "node:test";

import { parseSelection, selectionStorageKey } from "./conciergeSelection.ts";

test("storage key is namespaced per identity", () => {
  assert.equal(selectionStorageKey("abc"), "sprout-concierge.v1:abc");
});

test("parses a valid selection", () => {
  const got = parseSelection({ agentPubkey: "pk", updatedAt: 1717000000 });
  assert.deepEqual(got, { agentPubkey: "pk", updatedAt: 1717000000 });
});

test("rejects malformed payloads", () => {
  assert.equal(parseSelection(null), null);
  assert.equal(parseSelection("pk"), null);
  assert.equal(parseSelection({}), null);
  assert.equal(parseSelection({ agentPubkey: "", updatedAt: 1 }), null);
  assert.equal(parseSelection({ agentPubkey: "pk" }), null);
  assert.equal(
    parseSelection({ agentPubkey: "pk", updatedAt: Number.NaN }),
    null,
  );
});
