import assert from "node:assert/strict";
import test from "node:test";

import { pickMeshTarget, resolveConciergeAgent } from "./conciergeSession.ts";

const agent = (overrides) => ({
  name: "Concierge",
  pubkey: "pk",
  status: "stopped",
  updatedAt: "2026-06-01T00:00:00Z",
  ...overrides,
});

test("resolves the selected agent by pubkey regardless of name", () => {
  const jeeves = agent({ name: "Jeeves", pubkey: "a" });
  assert.equal(resolveConciergeAgent([jeeves], "a")?.pubkey, "a");
});

test("returns undefined without a selection", () => {
  assert.equal(resolveConciergeAgent([agent()], null), undefined);
});

test("returns undefined when the selected agent no longer exists", () => {
  assert.equal(resolveConciergeAgent([agent({ pubkey: "a" })], "b"), undefined);
});

test("never matches by name — only by pubkey", () => {
  const named = agent({ name: "Concierge", pubkey: "a" });
  assert.equal(resolveConciergeAgent([named], "other"), undefined);
});

test("pickMeshTarget takes the first serve target, or undefined", () => {
  assert.equal(pickMeshTarget([])?.modelId, undefined);
  assert.equal(
    pickMeshTarget([{ modelId: "m1" }, { modelId: "m2" }]).modelId,
    "m1",
  );
});
