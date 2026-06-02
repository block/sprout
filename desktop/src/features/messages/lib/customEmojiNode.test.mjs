import assert from "node:assert/strict";
import test from "node:test";

import { buildKnownShortcodeAlternation } from "./customEmojiNode.ts";

// The input rule converts a *completed* known `:shortcode:` into the atom
// node. `buildKnownShortcodeAlternation` produces the inner alternation; the
// rule wraps it as `(:(?:<alt>):)$`, case-insensitive. These tests exercise
// that wrapped pattern so they cover the actual matching behavior.

function ruleRegex(shortcodes) {
  const alt = buildKnownShortcodeAlternation(shortcodes);
  if (!alt) return null;
  return new RegExp(`(:(?:${alt}):)$`, "i");
}

test("returns null when there are no usable shortcodes", () => {
  assert.equal(buildKnownShortcodeAlternation([]), null);
  assert.equal(buildKnownShortcodeAlternation(["", "   "]), null);
});

test("matches a completed known shortcode at the input end", () => {
  const re = ruleRegex(["party_parrot"]);
  const m = re.exec("hello :party_parrot:");
  assert.ok(m);
  assert.equal(m[1], ":party_parrot:");
});

test("does not match an unknown shortcode", () => {
  const re = ruleRegex(["party_parrot"]);
  assert.equal(re.exec("typing :foo:"), null);
});

test("does not match an incomplete (unclosed) shortcode", () => {
  const re = ruleRegex(["party_parrot"]);
  // User mid-typing — no trailing colon yet.
  assert.equal(re.exec(":party_parro"), null);
});

test("only matches when the shortcode ends at the input position", () => {
  const re = ruleRegex(["wave"]);
  // `:wave:` is present but not at the end → the input rule shouldn't fire.
  assert.equal(re.exec(":wave: trailing"), null);
});

test("is case-insensitive on the shortcode", () => {
  const re = ruleRegex(["Wave"]);
  const m = re.exec(":WAVE:");
  assert.ok(m);
  assert.equal(m[1], ":WAVE:");
});

test("longest-first: a longer name is not shadowed by a shorter prefix", () => {
  const re = ruleRegex(["party", "party_parrot"]);
  const m = re.exec(":party_parrot:");
  assert.ok(m);
  // Must prefer the full `:party_parrot:`, not stop at `:party`.
  assert.equal(m[1], ":party_parrot:");
});

test("dedupes and ignores blank entries", () => {
  const alt = buildKnownShortcodeAlternation(["wave", "wave", "", "  "]);
  assert.equal(alt, "wave");
});
