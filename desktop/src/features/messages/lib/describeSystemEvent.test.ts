import { describe, expect, test } from "vitest";

import {
  describeSystemEvent,
  parseSystemMessagePayload,
} from "./describeSystemEvent";

// ── parseSystemMessagePayload ─────────────────────────────────────────

describe("parseSystemMessagePayload", () => {
  test("returns payload for valid JSON", () => {
    const result = parseSystemMessagePayload(
      '{"type":"member_joined","actor":"abc","target":"def"}',
    );
    expect(result).toEqual({
      type: "member_joined",
      actor: "abc",
      target: "def",
    });
  });

  test("returns null for invalid JSON", () => {
    expect(parseSystemMessagePayload("not json")).toBeNull();
  });

  test("returns null for empty string", () => {
    expect(parseSystemMessagePayload("")).toBeNull();
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
    expect(result).toMatch(/joined the channel/);
  });

  test("member_joined with different target shows 'added ... to the channel'", () => {
    const result = describeSystemEvent(
      { type: "member_joined", actor: "aaa", target: "bbb" },
      undefined,
      undefined,
    );
    expect(result).toMatch(/added .* to the channel/);
  });

  test("member_left shows 'left the channel'", () => {
    const result = describeSystemEvent(
      { type: "member_left", actor: "aaa" },
      undefined,
      undefined,
    );
    expect(result).toMatch(/left the channel/);
  });

  test("member_removed shows 'removed ... from the channel'", () => {
    const result = describeSystemEvent(
      { type: "member_removed", actor: "aaa", target: "bbb" },
      undefined,
      undefined,
    );
    expect(result).toMatch(/removed .* from the channel/);
  });

  test("topic_changed includes the topic text", () => {
    const result = describeSystemEvent(
      { type: "topic_changed", actor: "aaa", topic: "New Topic" },
      undefined,
      undefined,
    );
    expect(result).toMatch(/changed the topic to "New Topic"/);
  });

  test("purpose_changed includes the purpose text", () => {
    const result = describeSystemEvent(
      { type: "purpose_changed", actor: "aaa", purpose: "Ship stuff" },
      undefined,
      undefined,
    );
    expect(result).toMatch(/changed the purpose to "Ship stuff"/);
  });

  test("channel_created shows 'created this channel'", () => {
    const result = describeSystemEvent(
      { type: "channel_created", actor: "aaa" },
      undefined,
      undefined,
    );
    expect(result).toMatch(/created this channel/);
  });

  test("unknown type returns null", () => {
    const result = describeSystemEvent(
      { type: "unknown_type", actor: "aaa" },
      undefined,
      undefined,
    );
    expect(result).toBeNull();
  });

  test("currentPubkey resolves to 'You'", () => {
    const result = describeSystemEvent(
      { type: "member_left", actor: "aaa" },
      "aaa",
      undefined,
    );
    expect(result).toBe("You left the channel");
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
    expect(result).toMatch(/added Wes to the channel/);
  });

  test("missing actor shows 'Someone'", () => {
    const result = describeSystemEvent(
      { type: "channel_created" },
      undefined,
      undefined,
    );
    expect(result).toBe("Someone created this channel");
  });
});
