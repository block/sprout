import assert from "node:assert/strict";
import test from "node:test";

import { resolveDispatchChannel } from "./approveDispatch.ts";

const channel = (id, name) => ({ id, name });

test("resolves by name case-insensitively, ignoring # prefix and whitespace", () => {
  const channels = [channel("1", "general"), channel("2", "Engineering")];
  assert.equal(resolveDispatchChannel(channels, "engineering")?.id, "2");
  assert.equal(resolveDispatchChannel(channels, "#Engineering")?.id, "2");
  assert.equal(resolveDispatchChannel(channels, "  general ")?.id, "1");
});

test("returns undefined when no channel matches", () => {
  assert.equal(
    resolveDispatchChannel([channel("1", "general")], "ops"),
    undefined,
  );
});
