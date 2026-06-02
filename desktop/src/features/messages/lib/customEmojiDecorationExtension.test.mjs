import assert from "node:assert/strict";
import test from "node:test";

import { buildEmojiShortcodePattern } from "./customEmojiDecorationExtension.ts";

function matches(pattern, text) {
  if (!pattern) return [];
  pattern.lastIndex = 0;
  const out = [];
  let m = pattern.exec(text);
  while (m !== null) {
    out.push(m[0]);
    m = pattern.exec(text);
  }
  return out;
}

test("returns null when there are no known shortcodes", () => {
  assert.equal(buildEmojiShortcodePattern([]), null);
  assert.equal(buildEmojiShortcodePattern(["", "   "]), null);
});

test("matches a known shortcode wrapped in colons", () => {
  const p = buildEmojiShortcodePattern(["party_parrot"]);
  assert.deepEqual(matches(p, "hello :party_parrot: world"), [
    ":party_parrot:",
  ]);
});

test("does not match unknown shortcodes", () => {
  const p = buildEmojiShortcodePattern(["party_parrot"]);
  // `:foo:` is unknown — a user mid-typing shouldn't flicker an image.
  assert.deepEqual(matches(p, "typing :foo: here"), []);
});

test("longest-first: a longer name is not shadowed by a shorter prefix", () => {
  const p = buildEmojiShortcodePattern(["party", "party_parrot"]);
  // Must prefer the full `:party_parrot:`, not stop at `:party`.
  assert.deepEqual(matches(p, ":party_parrot:"), [":party_parrot:"]);
});

test("matches case-insensitively (mixed-case from manual typing / other clients)", () => {
  const p = buildEmojiShortcodePattern(["party_parrot"]);
  assert.deepEqual(matches(p, ":Party_Parrot:"), [":Party_Parrot:"]);
});

test("matches multiple occurrences in one string", () => {
  const p = buildEmojiShortcodePattern(["a", "b"]);
  assert.deepEqual(matches(p, ":a: and :b: and :a:"), [":a:", ":b:", ":a:"]);
});

test("escapes regex-special characters in shortcodes", () => {
  // Shortcodes are validated elsewhere, but the pattern must still be safe.
  const p = buildEmojiShortcodePattern(["c++"]);
  assert.deepEqual(matches(p, "see :c++: ok"), [":c++:"]);
  // The `+` must be literal, not a quantifier — `:cccc:` must NOT match.
  assert.deepEqual(matches(p, ":cccc:"), []);
});
