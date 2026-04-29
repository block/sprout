import assert from "node:assert/strict";
import { describe, test } from "node:test";

import {
  describeSystemEvent,
  parseSystemMessagePayload,
} from "./describeSystemEvent.ts";

// ── parseSystemMessagePayload ─────────────────────────────────────────

describe("parseSystemMessagePayload", () => {
  test("returns payload for valid JSON", () => {
    const result = parseSystemMessagePayload(
      '{"type":"member_joined","actor":"abc","target":"def"}',
    );
    assert.deepEqual(result, {
      type: "member_joined",
      actor: "abc",
      target: "def",
    });
  });

  test("returns null for invalid JSON", () => {
    assert.equal(parseSystemMessagePayload("not json"), null);
  });

  test("returns null for empty string", () => {
    assert.equal(parseSystemMessagePayload(""), null);
  });
});

// ── describeSystemEvent ───────────────────────────────────────────────

describe("describeSystemEvent", () => {
  test("member_joined self-join shows 'joined the channel'", () => {
    const result = describeSystemEvent(
      { type: "member_joined", actor: "aaa", target: "aaa" },
      undefined,
      undefined,
    );
    assert.match(result, /joined the channel/);
  });

  test("member_joined with different target shows 'added ... to the channel'", () => {
    const result = describeSystemEvent(
      { type: "member_joined", actor: "aaa", target: "bbb" },
      undefined,
      undefined,
    );
    assert.match(result, /added .* to the channel/);
  });

  test("member_left shows 'left the channel'", () => {
    const result = describeSystemEvent(
      { type: "member_left", actor: "aaa" },
      undefined,
      undefined,
    );
    assert.match(result, /left the channel/);
  });

  test("member_removed shows 'removed ... from the channel'", () => {
    const result = describeSystemEvent(
      { type: "member_removed", actor: "aaa", target: "bbb" },
      undefined,
      undefined,
    );
    assert.match(result, /removed .* from the channel/);
  });

  test("topic_changed includes the topic text", () => {
    const result = describeSystemEvent(
      { type: "topic_changed", actor: "aaa", topic: "New Topic" },
      undefined,
      undefined,
    );
    assert.match(result, /changed the topic to "New Topic"/);
  });

  test("purpose_changed includes the purpose text", () => {
    const result = describeSystemEvent(
      { type: "purpose_changed", actor: "aaa", purpose: "Ship stuff" },
      undefined,
      undefined,
    );
    assert.match(result, /changed the purpose to "Ship stuff"/);
  });

  test("channel_created shows 'created this channel'", () => {
    const result = describeSystemEvent(
      { type: "channel_created", actor: "aaa" },
      undefined,
      undefined,
    );
    assert.match(result, /created this channel/);
  });

  test("unknown type returns null", () => {
    const result = describeSystemEvent(
      { type: "unknown_type", actor: "aaa" },
      undefined,
      undefined,
    );
    assert.equal(result, null);
  });

  test("currentPubkey resolves to 'You'", () => {
    const result = describeSystemEvent(
      { type: "member_left", actor: "aaa" },
      "aaa",
      undefined,
    );
    assert.equal(result, "You left the channel");
  });

  test("profiles resolve display names", () => {
    const profiles = {
      bbb: { displayName: "Wes", avatarUrl: null, nip05Handle: null },
    };
    const result = describeSystemEvent(
      { type: "member_joined", actor: "aaa", target: "bbb" },
      undefined,
      profiles,
    );
    assert.match(result, /added Wes to the channel/);
  });

  test("missing actor shows 'Someone'", () => {
    const result = describeSystemEvent(
      { type: "channel_created" },
      undefined,
      undefined,
    );
    assert.equal(result, "Someone created this channel");
  });
});
