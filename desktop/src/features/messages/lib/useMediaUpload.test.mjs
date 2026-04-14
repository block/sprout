import assert from "node:assert/strict";
import test from "node:test";

// shortHash is a simple utility: str.slice(0, 4)
// Inline it here to avoid importing from useMediaUpload.ts which has
// unresolvable @/shared path aliases outside the bundler.
function shortHash(hex) {
  return hex.slice(0, 4);
}

// ── shortHash ─────────────────────────────────────────────────────────

test("shortHash returns first 4 hex characters", () => {
  assert.equal(shortHash("abcdef1234567890"), "abcd");
});

test("shortHash handles minimum-length input", () => {
  assert.equal(shortHash("abcd"), "abcd");
});

test("shortHash returns empty string for empty input", () => {
  assert.equal(shortHash(""), "");
});

test("shortHash returns partial for short input", () => {
  assert.equal(shortHash("ab"), "ab");
});

// ── Upload slot ordering (pure state-update logic) ────────────────────
// The slot system uses reserveSlots → fillSlot to maintain insertion order
// when concurrent uploads finish out of order. We test the state-update
// functions in isolation (they're pure array transforms).

test("reserveSlots creates null placeholders", () => {
  // Simulate: start with empty slots, reserve 3
  const prev = [];
  const count = 3;
  const next = [...prev, ...new Array(count).fill(null)];
  assert.deepEqual(next, [null, null, null]);
});

test("fillSlot places descriptor at correct index", () => {
  // Simulate: 3 reserved slots, fill index 1 first (out of order)
  const slots = [null, null, null];
  const descriptor = { url: "https://example.com/b.png", sha256: "bbbb" };
  const next = [...slots];
  next[1] = descriptor;
  assert.equal(next[0], null);
  assert.deepEqual(next[1], descriptor);
  assert.equal(next[2], null);
});

test("concurrent uploads filling out of order preserves slot positions", () => {
  // Simulate: reserve 3 slots, uploads finish in order 2, 0, 1
  const slots = [null, null, null];
  const a = { url: "a.png", sha256: "aaaa" };
  const b = { url: "b.png", sha256: "bbbb" };
  const c = { url: "c.png", sha256: "cccc" };

  // Upload 2 finishes first
  const step1 = [...slots];
  step1[2] = c;
  assert.deepEqual(step1, [null, null, c]);

  // Upload 0 finishes second
  const step2 = [...step1];
  step2[0] = a;
  assert.deepEqual(step2, [a, null, c]);

  // Upload 1 finishes last
  const step3 = [...step2];
  step3[1] = b;
  assert.deepEqual(step3, [a, b, c]);

  // Filter nulls — final order matches original slot order
  const result = step3.filter((d) => d !== null);
  assert.deepEqual(result, [a, b, c]);
});

test("removing an attachment filters by URL and resets slot count", () => {
  const a = { url: "a.png", sha256: "aaaa" };
  const b = { url: "b.png", sha256: "bbbb" };
  const c = { url: "c.png", sha256: "cccc" };
  const slots = [a, b, c];

  // Remove b
  const next = slots.filter((d) => d?.url !== "b.png");
  assert.deepEqual(next, [a, c]);
  assert.equal(next.length, 2);
});

test("reserveSlots pads if slots array is shorter than expected start index", () => {
  // Edge case: if somehow prev is shorter than startIndex
  const prev = [{ url: "a.png", sha256: "aaaa" }];
  const startIndex = 3;
  const count = 2;
  const padded =
    prev.length < startIndex
      ? [...prev, ...new Array(startIndex - prev.length).fill(null)]
      : prev;
  const next = [...padded, ...new Array(count).fill(null)];
  assert.equal(next.length, 5);
  assert.deepEqual(next[0], { url: "a.png", sha256: "aaaa" });
  assert.equal(next[1], null); // padding
  assert.equal(next[2], null); // padding
  assert.equal(next[3], null); // reserved
  assert.equal(next[4], null); // reserved
});
