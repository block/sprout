import assert from "node:assert/strict";
import test from "node:test";

import { findConciergeAgent, pickMeshTarget } from "./conciergeSession.ts";

const agent = (overrides) => ({
  name: "Concierge",
  pubkey: "pk",
  status: "stopped",
  updatedAt: "2026-06-01T00:00:00Z",
  ...overrides,
});

test("matches by name case-insensitively with surrounding whitespace", () => {
  const match = findConciergeAgent([agent({ name: " concierge " })]);
  assert.ok(match);
});

test("ignores other agents", () => {
  assert.equal(findConciergeAgent([agent({ name: "Max" })]), undefined);
  assert.equal(findConciergeAgent([]), undefined);
});

test("prefers running agents over newer stopped ones", () => {
  const stoppedNewer = agent({
    pubkey: "a",
    updatedAt: "2026-06-08T00:00:00Z",
  });
  const runningOlder = agent({
    pubkey: "b",
    status: "running",
    updatedAt: "2026-06-01T00:00:00Z",
  });
  assert.equal(findConciergeAgent([stoppedNewer, runningOlder])?.pubkey, "b");
});

test("falls back to most recently updated among equals", () => {
  const older = agent({ pubkey: "a", updatedAt: "2026-06-01T00:00:00Z" });
  const newer = agent({ pubkey: "b", updatedAt: "2026-06-08T00:00:00Z" });
  assert.equal(findConciergeAgent([older, newer])?.pubkey, "b");
});

test("pickMeshTarget takes the first serve target, or undefined", () => {
  assert.equal(pickMeshTarget([])?.modelId, undefined);
  assert.equal(
    pickMeshTarget([{ modelId: "m1" }, { modelId: "m2" }]).modelId,
    "m1",
  );
});
