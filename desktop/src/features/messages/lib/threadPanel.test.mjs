import assert from "node:assert/strict";
import test from "node:test";

import { buildMainTimelineEntries } from "./threadPanel.ts";

const KIND_STREAM_MESSAGE_DIFF = 40008;

function message(overrides) {
  return {
    id: "message",
    createdAt: 1,
    pubkey: "author",
    author: "Author",
    avatarUrl: null,
    role: undefined,
    personaDisplayName: undefined,
    time: "12:00 PM",
    body: "body",
    parentId: null,
    rootId: null,
    depth: 0,
    accent: false,
    pending: undefined,
    edited: false,
    kind: 9,
    tags: [],
    reactions: undefined,
    ...overrides,
  };
}

test("buildMainTimelineEntries includes broadcast replies", () => {
  const root = message({ id: "root", createdAt: 1 });
  const hiddenReply = message({
    id: "hidden-reply",
    createdAt: 2,
    parentId: "root",
    rootId: "root",
    depth: 1,
    tags: [["e", "root", "", "reply"]],
  });
  const broadcastReply = message({
    id: "broadcast-reply",
    createdAt: 3,
    parentId: "root",
    rootId: "root",
    depth: 1,
    tags: [
      ["e", "root", "", "reply"],
      ["broadcast", "1"],
    ],
  });

  assert.deepEqual(
    buildMainTimelineEntries([root, hiddenReply, broadcastReply]).map(
      (entry) => entry.message.id,
    ),
    ["root", "broadcast-reply"],
  );
});

test("buildMainTimelineEntries excludes diff artifacts from reply summaries", () => {
  const root = message({ id: "root", createdAt: 1 });
  const firstReply = message({
    id: "first-reply",
    createdAt: 2,
    parentId: "root",
    rootId: "root",
    tags: [["e", "root", "", "reply"]],
  });
  const diffArtifact = message({
    id: "diff-artifact",
    createdAt: 3,
    kind: KIND_STREAM_MESSAGE_DIFF,
    parentId: "root",
    rootId: "root",
    tags: [["e", "root", "", "reply"]],
  });
  const secondReply = message({
    id: "second-reply",
    createdAt: 4,
    parentId: "root",
    rootId: "root",
    tags: [["e", "root", "", "reply"]],
  });

  const entries = buildMainTimelineEntries([
    root,
    firstReply,
    diffArtifact,
    secondReply,
  ]);

  assert.equal(entries[0]?.summary?.replyCount, 2);
  assert.deepEqual(
    entries[0]?.summary?.participants.map((participant) => participant.id),
    ["author"],
  );
});
