import assert from "node:assert/strict";
import test from "node:test";

/**
 * Pure extraction of the ProseMirror → plain-text cursor mapping logic
 * from getTextAndCursor in useRichTextEditor.ts.
 *
 * Takes a list of "visited nodes" (as the descendants callback would see them)
 * and a ProseMirror anchor position, returns the plain-text offset.
 */
function mapAnchorToPlainText(nodes, anchor) {
  let offset = 0;
  let found = false;
  for (const { isText, isBlock, pos, nodeSize } of nodes) {
    if (found) break;
    if (isText) {
      const nodeEnd = pos + nodeSize;
      if (anchor <= nodeEnd) {
        offset += anchor - pos;
        found = true;
        break;
      }
      offset += nodeSize;
    } else if (isBlock && pos > 0) {
      offset += 1;
    }
  }
  return found ? offset : -1; // -1 means "fell through"
}

// ── Single paragraph ──────────────────────────────────────────────────

test("cursor at start of single paragraph", () => {
  // doc > paragraph(pos=0) > text "hello"(pos=1, size=5)
  const nodes = [
    { isText: false, isBlock: true, pos: 0, nodeSize: 7 },
    { isText: true, isBlock: false, pos: 1, nodeSize: 5 },
  ];
  // Anchor at pos=1 → plain-text offset 0
  assert.equal(mapAnchorToPlainText(nodes, 1), 0);
});

test("cursor at end of single paragraph", () => {
  const nodes = [
    { isText: false, isBlock: true, pos: 0, nodeSize: 7 },
    { isText: true, isBlock: false, pos: 1, nodeSize: 5 },
  ];
  // Anchor at pos=6 → plain-text offset 5
  assert.equal(mapAnchorToPlainText(nodes, 6), 5);
});

test("cursor mid-word in single paragraph", () => {
  const nodes = [
    { isText: false, isBlock: true, pos: 0, nodeSize: 7 },
    { isText: true, isBlock: false, pos: 1, nodeSize: 5 },
  ];
  // Anchor at pos=3 → plain-text offset 2 (after "he")
  assert.equal(mapAnchorToPlainText(nodes, 3), 2);
});

// ── Two paragraphs (the bug scenario) ─────────────────────────────────
// doc structure: doc > p1("hello") > p2("world")
// ProseMirror positions: doc=0, p1=0, "hello"=1..5, /p1=6, p2=7, "world"=8..12, /p2=13
// textContent = "hello\nworld" (11 chars)

test("cursor in second paragraph accounts for block boundary newline", () => {
  const nodes = [
    { isText: false, isBlock: true, pos: 0, nodeSize: 7 }, // p1
    { isText: true, isBlock: false, pos: 1, nodeSize: 5 }, // "hello"
    { isText: false, isBlock: true, pos: 7, nodeSize: 7 }, // p2 (pos > 0 → newline)
    { isText: true, isBlock: false, pos: 8, nodeSize: 5 }, // "world"
  ];
  // Anchor at pos=8 → start of "world" → plain-text offset 6 ("hello\n" = 6 chars)
  assert.equal(mapAnchorToPlainText(nodes, 8), 6);
});

test("cursor mid-word in second paragraph", () => {
  const nodes = [
    { isText: false, isBlock: true, pos: 0, nodeSize: 7 },
    { isText: true, isBlock: false, pos: 1, nodeSize: 5 },
    { isText: false, isBlock: true, pos: 7, nodeSize: 7 },
    { isText: true, isBlock: false, pos: 8, nodeSize: 5 },
  ];
  // Anchor at pos=10 → "wo|rld" → plain-text offset 8 ("hello\nwo" = 8 chars)
  assert.equal(mapAnchorToPlainText(nodes, 10), 8);
});

// ── Three paragraphs (cumulative drift) ───────────────────────────────
// "aaa\nbbb\nccc" — without the fix, offset would drift by 1 per boundary

test("cursor in third paragraph accounts for two block boundaries", () => {
  const nodes = [
    { isText: false, isBlock: true, pos: 0, nodeSize: 5 }, // p1
    { isText: true, isBlock: false, pos: 1, nodeSize: 3 }, // "aaa"
    { isText: false, isBlock: true, pos: 5, nodeSize: 5 }, // p2
    { isText: true, isBlock: false, pos: 6, nodeSize: 3 }, // "bbb"
    { isText: false, isBlock: true, pos: 10, nodeSize: 5 }, // p3
    { isText: true, isBlock: false, pos: 11, nodeSize: 3 }, // "ccc"
  ];
  // Anchor at pos=11 → start of "ccc" → plain-text offset 8 ("aaa\nbbb\n" = 8 chars)
  assert.equal(mapAnchorToPlainText(nodes, 11), 8);
});

test("cursor at end of third paragraph", () => {
  const nodes = [
    { isText: false, isBlock: true, pos: 0, nodeSize: 5 },
    { isText: true, isBlock: false, pos: 1, nodeSize: 3 },
    { isText: false, isBlock: true, pos: 5, nodeSize: 5 },
    { isText: true, isBlock: false, pos: 6, nodeSize: 3 },
    { isText: false, isBlock: true, pos: 10, nodeSize: 5 },
    { isText: true, isBlock: false, pos: 11, nodeSize: 3 },
  ];
  // Anchor at pos=14 → end of "ccc" → plain-text offset 11 ("aaa\nbbb\nccc" = 11 chars)
  assert.equal(mapAnchorToPlainText(nodes, 14), 11);
});

// ── First paragraph is unaffected (pos === 0, no newline) ─────────────

test("first block boundary at pos 0 does not add newline", () => {
  const nodes = [
    { isText: false, isBlock: true, pos: 0, nodeSize: 7 },
    { isText: true, isBlock: false, pos: 1, nodeSize: 5 },
  ];
  // Anchor at pos=4 → plain-text offset 3 (no extra newline for first block)
  assert.equal(mapAnchorToPlainText(nodes, 4), 3);
});
