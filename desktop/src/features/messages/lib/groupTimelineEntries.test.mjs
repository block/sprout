import assert from "node:assert/strict";
import { describe, test } from "node:test";

import { groupTimelineEntries } from "./groupTimelineEntries.ts";

// ── Helpers ───────────────────────────────────────────────────────────

const KIND_SYSTEM_MESSAGE = 40099;
const KIND_CHAT = 9;

function makeEntry(overrides = {}) {
  return {
    message: {
      id: `msg-${Math.random().toString(36).slice(2)}`,
      createdAt: 1000,
      pubkey: "author1",
      author: "Author 1",
      time: "12:00",
      body: "hello",
      depth: 0,
      kind: KIND_CHAT,
      ...overrides,
    },
    summary: null,
  };
}

function makeSystemEntry(type, actor, target) {
  return makeEntry({
    kind: KIND_SYSTEM_MESSAGE,
    body: JSON.stringify({ type, actor, target }),
  });
}

// ── Empty input ───────────────────────────────────────────────────────

describe("groupTimelineEntries", () => {
  test("returns empty array for empty input", () => {
    assert.deepEqual(groupTimelineEntries([]), []);
  });

  // ── Single entries (no grouping) ────────────────────────────────────

  test("single chat message is not compacted", () => {
    const entry = makeEntry();
    const result = groupTimelineEntries([entry]);
    assert.equal(result.length, 1);
    assert.equal(result[0].entryType, "message");
    if (result[0].entryType === "message") {
      assert.equal(result[0].isGroupContinuation, false);
    }
  });

  test("single system event renders as normal message (no accordion)", () => {
    const entry = makeSystemEntry("member_joined", "a", "b");
    const result = groupTimelineEntries([entry]);
    assert.equal(result.length, 1);
    assert.equal(result[0].entryType, "message");
  });

  // ── System event grouping ───────────────────────────────────────────

  test("2+ consecutive system events form a group", () => {
    const entries = [
      makeSystemEntry("member_joined", "a", "b"),
      makeSystemEntry("member_joined", "a", "c"),
    ];
    const result = groupTimelineEntries(entries);
    assert.equal(result.length, 1);
    assert.equal(result[0].entryType, "system-event-group");
    if (result[0].entryType === "system-event-group") {
      assert.equal(result[0].entries.length, 2);
    }
  });

  test("system events separated by a chat message form separate groups", () => {
    const entries = [
      makeSystemEntry("member_joined", "a", "b"),
      makeSystemEntry("member_joined", "a", "c"),
      makeEntry({ createdAt: 1001 }),
      makeSystemEntry("member_left", "d"),
      makeSystemEntry("member_left", "e"),
    ];
    const result = groupTimelineEntries(entries);
    assert.equal(result.length, 3);
    assert.equal(result[0].entryType, "system-event-group");
    assert.equal(result[1].entryType, "message");
    assert.equal(result[2].entryType, "system-event-group");
  });

  // ── Message compacting ──────────────────────────────────────────────

  test("same author within 2 min is compacted", () => {
    const entries = [
      makeEntry({ createdAt: 1000, pubkey: "a" }),
      makeEntry({ createdAt: 1060, pubkey: "a" }),
    ];
    const result = groupTimelineEntries(entries);
    assert.equal(result.length, 2);
    if (result[0].entryType === "message") {
      assert.equal(result[0].isGroupContinuation, false);
    }
    if (result[1].entryType === "message") {
      assert.equal(result[1].isGroupContinuation, true);
    }
  });

  test("same author beyond 2 min is NOT compacted", () => {
    const entries = [
      makeEntry({ createdAt: 1000, pubkey: "a" }),
      makeEntry({ createdAt: 1121, pubkey: "a" }), // 121 seconds > 120
    ];
    const result = groupTimelineEntries(entries);
    if (result[1].entryType === "message") {
      assert.equal(result[1].isGroupContinuation, false);
    }
  });

  test("different authors are NOT compacted", () => {
    const entries = [
      makeEntry({ createdAt: 1000, pubkey: "a" }),
      makeEntry({ createdAt: 1010, pubkey: "b" }),
    ];
    const result = groupTimelineEntries(entries);
    if (result[1].entryType === "message") {
      assert.equal(result[1].isGroupContinuation, false);
    }
  });

  test("message with thread summary breaks compacting", () => {
    const entries = [
      makeEntry({ createdAt: 1000, pubkey: "a" }),
      {
        message: {
          id: "msg-2",
          createdAt: 1010,
          pubkey: "a",
          author: "Author",
          time: "12:00",
          body: "reply",
          depth: 0,
          kind: KIND_CHAT,
        },
        summary: {
          threadHeadId: "msg-1",
          replyCount: 3,
          participants: [],
        },
      },
    ];
    const result = groupTimelineEntries(entries);
    if (result[1].entryType === "message") {
      assert.equal(result[1].isGroupContinuation, false);
    }
  });

  test("system message after chat message breaks compacting", () => {
    const entries = [
      makeEntry({ createdAt: 1000, pubkey: "a" }),
      makeSystemEntry("topic_changed", "a"),
    ];
    const result = groupTimelineEntries(entries);
    assert.equal(result[1].entryType, "message");
    if (result[1].entryType === "message") {
      assert.equal(result[1].isGroupContinuation, false);
    }
  });

  // ── Boundary: exactly 120 seconds ───────────────────────────────────

  test("exactly 120 seconds gap is still compacted", () => {
    const entries = [
      makeEntry({ createdAt: 1000, pubkey: "a" }),
      makeEntry({ createdAt: 1120, pubkey: "a" }), // exactly 120
    ];
    const result = groupTimelineEntries(entries);
    if (result[1].entryType === "message") {
      assert.equal(result[1].isGroupContinuation, true);
    }
  });
});
