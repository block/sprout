import assert from "node:assert/strict";
import test from "node:test";

import {
  applySettledStatus,
  dispatchStorageKey,
  parseDispatchIntents,
  readSettledDispatches,
} from "./dispatchIntent.ts";

// --- parseDispatchIntents ---

test("parses a single dispatch fence and strips it from the content", () => {
  const content = [
    "Sure — I'll have Max watch CI.",
    "```dispatch",
    '{"agent": "Max", "channel": "engineering", "instruction": "Watch CI on the relay PR."}',
    "```",
  ].join("\n");
  const { intents, cleanedContent } = parseDispatchIntents("m1", content);
  assert.equal(intents.length, 1);
  assert.deepEqual(intents[0], {
    id: "m1:0",
    agent: "Max",
    channel: "engineering",
    instruction: "Watch CI on the relay PR.",
    status: "pending",
  });
  assert.equal(cleanedContent, "Sure — I'll have Max watch CI.");
});

test("normalizes @agent and #channel prefixes", () => {
  const content =
    '```dispatch\n{"agent": "@Max", "channel": "#engineering", "instruction": "go"}\n```';
  const { intents } = parseDispatchIntents("m1", content);
  assert.equal(intents[0].agent, "Max");
  assert.equal(intents[0].channel, "engineering");
});

test("drops malformed JSON blocks fail-closed but keeps valid ones, with stable ids", () => {
  const content = [
    "```dispatch",
    "{not json",
    "```",
    "```dispatch",
    '{"agent": "Sami", "channel": "ops", "instruction": "deploy"}',
    "```",
  ].join("\n");
  const { intents, cleanedContent } = parseDispatchIntents("m9", content);
  assert.equal(intents.length, 1);
  // index advances past the malformed block, so settled-state keys stay stable
  assert.equal(intents[0].id, "m9:1");
  assert.equal(cleanedContent, "");
});

test("drops blocks with missing or blank required fields", () => {
  for (const body of [
    '{"agent": "Max", "channel": "eng"}',
    '{"agent": "", "channel": "eng", "instruction": "x"}',
    '{"agent": "Max", "channel": "eng", "instruction": "   "}',
  ]) {
    const { intents } = parseDispatchIntents(
      "m1",
      `\`\`\`dispatch\n${body}\n\`\`\``,
    );
    assert.equal(intents.length, 0, body);
  }
});

test("leaves non-dispatch code fences untouched", () => {
  const content = "Here:\n```python\nprint('hi')\n```";
  const { intents, cleanedContent } = parseDispatchIntents("m1", content);
  assert.equal(intents.length, 0);
  assert.equal(cleanedContent, content);
});

// --- settled-state persistence ---

test("dispatchStorageKey is namespaced per identity, case-normalized", () => {
  assert.equal(
    dispatchStorageKey("ABCDEF"),
    "sprout-concierge-dispatch.v1:abcdef",
  );
});

test("readSettledDispatches tolerates null, garbage, and unknown statuses", () => {
  assert.deepEqual(readSettledDispatches(null), {});
  assert.deepEqual(readSettledDispatches("{not json"), {});
  assert.deepEqual(
    readSettledDispatches(
      '{"a:0": "approved", "b:1": "weird", "c:2": "dismissed"}',
    ),
    { "a:0": "approved", "c:2": "dismissed" },
  );
});

test("applySettledStatus overrides status only when settled", () => {
  const intent = {
    id: "m1:0",
    agent: "Max",
    channel: "eng",
    instruction: "go",
    status: "pending",
  };
  assert.equal(applySettledStatus(intent, {}).status, "pending");
  assert.equal(
    applySettledStatus(intent, { "m1:0": "approved" }).status,
    "approved",
  );
  // does not mutate the input
  assert.equal(intent.status, "pending");
});
