import assert from "node:assert/strict";
import test from "node:test";

// ── Inlined pure functions from imetaMediaMarkdown.ts ─────────────────
// Inlined to avoid importing from .ts files (no TS loader in node:test).
// Same pattern as markdown.test.mjs / useMediaUpload.test.mjs.

const MEDIA_LINE_RE = /^!\[(?:image|video)\]\(([^)\s]+)\)\s*$/;

function stripImetaMediaLines(body, imetaMedia) {
  if (imetaMedia.length === 0) return body;
  const urls = new Set(imetaMedia.map((m) => m.url));
  const lines = body.split("\n");
  let end = lines.length;
  while (end > 0) {
    const line = lines[end - 1];
    if (line.trim() === "") {
      end -= 1;
      continue;
    }
    const match = line.match(MEDIA_LINE_RE);
    if (match && urls.has(match[1])) {
      end -= 1;
      continue;
    }
    break;
  }
  return lines.slice(0, end).join("\n").replace(/\s+$/, "");
}

function appendImetaMediaLines(body, imetaMedia) {
  if (imetaMedia.length === 0) return body;
  let out = body;
  for (const { url, m } of imetaMedia) {
    if (out.includes(url)) continue;
    const isVideo = m.startsWith("video/");
    out += isVideo ? `\n![video](${url})` : `\n![image](${url})`;
  }
  return out;
}

// Convenience: full edit round-trip — strip, user edits text, re-append.
function editRoundTrip(originalBody, imetaMedia, newText) {
  const editable = stripImetaMediaLines(originalBody, imetaMedia);
  void editable; // mirror composer flow; result not asserted here
  return appendImetaMediaLines(newText, imetaMedia);
}

// ── stripImetaMediaLines ──────────────────────────────────────────────

test("strip: removes trailing image line whose URL is in imetaMedia", () => {
  const body = "Look at this\n![image](https://blossom/abc.png)";
  const stripped = stripImetaMediaLines(body, [
    { url: "https://blossom/abc.png", m: "image/png" },
  ]);
  assert.equal(stripped, "Look at this");
});

test("strip: removes trailing video line", () => {
  const body = "Demo:\n![video](https://blossom/clip.mp4)";
  const stripped = stripImetaMediaLines(body, [
    { url: "https://blossom/clip.mp4", m: "video/mp4" },
  ]);
  assert.equal(stripped, "Demo:");
});

test("strip: removes multiple trailing media lines in order", () => {
  const body = "two pics\n![image](https://b/a.png)\n![image](https://b/b.png)";
  const stripped = stripImetaMediaLines(body, [
    { url: "https://b/a.png", m: "image/png" },
    { url: "https://b/b.png", m: "image/png" },
  ]);
  assert.equal(stripped, "two pics");
});

test("strip: leaves body alone when no imeta entries", () => {
  const body = "hello\n![image](https://b/a.png)";
  assert.equal(stripImetaMediaLines(body, []), body);
});

test("strip: leaves media line whose URL isn't in imetaMedia", () => {
  const body = "hello\n![image](https://b/other.png)";
  const stripped = stripImetaMediaLines(body, [
    { url: "https://b/known.png", m: "image/png" },
  ]);
  assert.equal(stripped, body);
});

test("strip: stops at first non-media line (interleaved text preserved)", () => {
  const body =
    "before\n![image](https://b/a.png)\nmiddle\n![image](https://b/b.png)";
  const stripped = stripImetaMediaLines(body, [
    { url: "https://b/a.png", m: "image/png" },
    { url: "https://b/b.png", m: "image/png" },
  ]);
  // Only the trailing one is peeled off; interleaved text stays.
  assert.equal(stripped, "before\n![image](https://b/a.png)\nmiddle");
});

test("strip: tolerates blank lines between text and trailing media", () => {
  const body = "hi\n\n![image](https://b/a.png)";
  const stripped = stripImetaMediaLines(body, [
    { url: "https://b/a.png", m: "image/png" },
  ]);
  assert.equal(stripped, "hi");
});

// ── appendImetaMediaLines ─────────────────────────────────────────────

test("append: adds image and video lines based on mime type", () => {
  const out = appendImetaMediaLines("new caption", [
    { url: "https://b/a.png", m: "image/png" },
    { url: "https://b/c.mp4", m: "video/mp4" },
  ]);
  assert.equal(
    out,
    "new caption\n![image](https://b/a.png)\n![video](https://b/c.mp4)",
  );
});

test("append: no-op when imetaMedia empty", () => {
  assert.equal(appendImetaMediaLines("hi", []), "hi");
});

// ── Round-trip (the bug fix) ──────────────────────────────────────────

test("edit-of-media-message: final body still contains all imeta URLs", () => {
  const originalBody =
    "old caption\n![image](https://b/photo.png)\n![video](https://b/clip.mp4)";
  const imetaMedia = [
    { url: "https://b/photo.png", m: "image/png" },
    { url: "https://b/clip.mp4", m: "video/mp4" },
  ];

  // User opens edit — composer strips media lines.
  const editable = stripImetaMediaLines(originalBody, imetaMedia);
  assert.equal(editable, "old caption");

  // User changes the text and saves.
  const finalBody = appendImetaMediaLines("new caption", imetaMedia);

  for (const { url } of imetaMedia) {
    assert.ok(
      finalBody.includes(url),
      `final body should still contain ${url}`,
    );
  }
  // Order preserved; alt label matches mime type.
  assert.equal(
    finalBody,
    "new caption\n![image](https://b/photo.png)\n![video](https://b/clip.mp4)",
  );
});

test("edit-of-media-message: video URL without .mp4 suffix still rendered as video", () => {
  // markdown.tsx switches on .mp4 suffix today, but the alt text we emit is
  // mime-driven, so a videos served from a CDN-style URL (no extension)
  // round-trip with the right `![video]` label.
  const imetaMedia = [{ url: "https://cdn/blob/xyz", m: "video/mp4" }];
  const finalBody = editRoundTrip(
    "caption\n![video](https://cdn/blob/xyz)",
    imetaMedia,
    "new caption",
  );
  assert.equal(finalBody, "new caption\n![video](https://cdn/blob/xyz)");
});

test("edit-of-text-only-message: no imeta, body unchanged shape", () => {
  const finalBody = editRoundTrip("hello", [], "world");
  assert.equal(finalBody, "world");
});

test("append: empty body + imeta produces non-empty media-only body", () => {
  // Backs the "media-only-no-caption edit" submit path: when the user clears
  // the caption, finalContent must still contain the imeta URLs so the saved
  // event renders the attachment.
  const out = appendImetaMediaLines("", [
    { url: "https://b/a.png", m: "image/png" },
  ]);
  assert.equal(out, "\n![image](https://b/a.png)");
  assert.ok(out.includes("https://b/a.png"));
});

// ── append: dedup against URL already present in body ────────────────

test("append: skips URL already textually present in body", () => {
  // Defends non-trailing layouts: if strip leaves a media line in place
  // (e.g. interleaved with text), append must not re-add the same URL.
  const body = "before\n![image](https://b/a.png)\nmiddle";
  const out = appendImetaMediaLines(body, [
    { url: "https://b/a.png", m: "image/png" },
  ]);
  assert.equal(out, body);
});

test("append: dedup is per-URL (other entries still appended)", () => {
  const body = "x\n![image](https://b/a.png)\ny";
  const out = appendImetaMediaLines(body, [
    { url: "https://b/a.png", m: "image/png" },
    { url: "https://b/new.png", m: "image/png" },
  ]);
  assert.equal(
    out,
    "x\n![image](https://b/a.png)\ny\n![image](https://b/new.png)",
  );
});

test("round-trip: interleaved imeta layout doesn't duplicate on save", () => {
  // strip leaves the interleaved line in place, append must skip it.
  const originalBody = "before\n![image](https://b/a.png)\nmiddle";
  const imetaMedia = [{ url: "https://b/a.png", m: "image/png" }];
  const editable = stripImetaMediaLines(originalBody, imetaMedia);
  // Editable content still contains the embedded media line (out of scope to
  // strip); user edits the surrounding text and saves.
  assert.equal(editable, originalBody);
  const finalBody = appendImetaMediaLines(editable, imetaMedia);
  assert.equal(finalBody, originalBody);
});
