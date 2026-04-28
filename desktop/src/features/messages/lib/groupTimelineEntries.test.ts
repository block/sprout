import { describe, expect, test } from "vitest";

import type { MainTimelineEntry } from "./threadPanel";
import { groupTimelineEntries } from "./groupTimelineEntries";

// ── Helpers ───────────────────────────────────────────────────────────

const KIND_SYSTEM_MESSAGE = 40099;
const KIND_CHAT = 9;

function makeEntry(
  overrides: Partial<MainTimelineEntry["message"]> = {},
): MainTimelineEntry {
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

function makeSystemEntry(
  type: string,
  actor: string,
  target?: string,
): MainTimelineEntry {
  return makeEntry({
    kind: KIND_SYSTEM_MESSAGE,
    body: JSON.stringify({ type, actor, target }),
  });
}

// ── Empty input ───────────────────────────────────────────────────────

describe("groupTimelineEntries", () => {
  test("returns empty array for empty input", () => {
    expect(groupTimelineEntries([])).toEqual([]);
  });

  // ── Single entries (no grouping) ────────────────────────────────────

  test("single chat message is not compacted", () => {
    const entry = makeEntry();
    const result = groupTimelineEntries([entry]);
    expect(result).toHaveLength(1);
    expect(result[0].entryType).toBe("message");
    if (result[0].entryType === "message") {
      expect(result[0].isGroupContinuation).toBe(false);
    }
  });

  test("single system event renders as normal message (no accordion)", () => {
    const entry = makeSystemEntry("member_joined", "a", "b");
    const result = groupTimelineEntries([entry]);
    expect(result).toHaveLength(1);
    expect(result[0].entryType).toBe("message");
  });

  // ── System event grouping ───────────────────────────────────────────

  test("2+ consecutive system events form a group", () => {
    const entries = [
      makeSystemEntry("member_joined", "a", "b"),
      makeSystemEntry("member_joined", "a", "c"),
    ];
    const result = groupTimelineEntries(entries);
    expect(result).toHaveLength(1);
    expect(result[0].entryType).toBe("system-event-group");
    if (result[0].entryType === "system-event-group") {
      expect(result[0].entries).toHaveLength(2);
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
    expect(result).toHaveLength(3);
    expect(result[0].entryType).toBe("system-event-group");
    expect(result[1].entryType).toBe("message");
    expect(result[2].entryType).toBe("system-event-group");
  });

  // ── Message compacting ──────────────────────────────────────────────

  test("same author within 2 min is compacted", () => {
    const entries = [
      makeEntry({ createdAt: 1000, pubkey: "a" }),
      makeEntry({ createdAt: 1060, pubkey: "a" }),
    ];
    const result = groupTimelineEntries(entries);
    expect(result).toHaveLength(2);
    if (result[0].entryType === "message") {
      expect(result[0].isGroupContinuation).toBe(false);
    }
    if (result[1].entryType === "message") {
      expect(result[1].isGroupContinuation).toBe(true);
    }
  });

  test("same author beyond 2 min is NOT compacted", () => {
    const entries = [
      makeEntry({ createdAt: 1000, pubkey: "a" }),
      makeEntry({ createdAt: 1121, pubkey: "a" }), // 121 seconds > 120
    ];
    const result = groupTimelineEntries(entries);
    if (result[1].entryType === "message") {
      expect(result[1].isGroupContinuation).toBe(false);
    }
  });

  test("different authors are NOT compacted", () => {
    const entries = [
      makeEntry({ createdAt: 1000, pubkey: "a" }),
      makeEntry({ createdAt: 1010, pubkey: "b" }),
    ];
    const result = groupTimelineEntries(entries);
    if (result[1].entryType === "message") {
      expect(result[1].isGroupContinuation).toBe(false);
    }
  });

  test("message with thread summary breaks compacting", () => {
    const entries: MainTimelineEntry[] = [
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
      expect(result[1].isGroupContinuation).toBe(false);
    }
  });

  test("system message after chat message breaks compacting", () => {
    const entries = [
      makeEntry({ createdAt: 1000, pubkey: "a" }),
      makeSystemEntry("topic_changed", "a"),
    ];
    const result = groupTimelineEntries(entries);
    expect(result[1].entryType).toBe("message");
    if (result[1].entryType === "message") {
      expect(result[1].isGroupContinuation).toBe(false);
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
      expect(result[1].isGroupContinuation).toBe(true);
    }
  });
});
