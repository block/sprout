import assert from "node:assert/strict";
import test from "node:test";

// ── Inlined pure logic from mediaUrl.ts ───────────────────────────────
//
// We can't import mediaUrl.ts directly in a .mjs test file — it depends on
// @tauri-apps/api/core (Tauri IPC), which isn't available outside the bundler.
// Same pattern as the other test files in this project: inline the pure
// functions under test and verify their behaviour directly.

// Matches: https://anything.com/media/{64-hex}.{ext}
// Also matches thumbnails: /media/{64-hex}.thumb.jpg
const RELAY_MEDIA_RE =
  /^(?:https?:\/\/[^/]+)\/media\/([\da-f]{64}(?:\.thumb)?\.(?:jpg|png|gif|webp|mp4)(?:\?.*)?)$/;

/**
 * Check if a media URL belongs to our relay. Matches if:
 * 1. The URL starts with the relay origin (exact match), OR
 * 2. The URL's hostname shares the same base domain as the relay
 *    (e.g. "sprout-relay-production.up.railway.app" matches relay origin
 *    "https://sprout.up.railway.app" because both are *.up.railway.app).
 */
function isRelayMediaOrigin(url, relayOrigin) {
  if (url.startsWith(`${relayOrigin}/`)) {
    return true;
  }

  try {
    const urlHost = new URL(url).hostname;
    const relayHost = new URL(relayOrigin).hostname;

    if (urlHost === relayHost) return true;

    const urlParts = urlHost.split(".");
    const relayParts = relayHost.split(".");

    if (urlParts.length >= 3 && relayParts.length >= 3) {
      const urlParent = urlParts.slice(1).join(".");
      const relayParent = relayParts.slice(1).join(".");
      if (urlParent === relayParent) return true;
    }
  } catch {
    // URL parsing failed — fall through to proxy (safe default)
    return true;
  }

  return false;
}

/**
 * Synchronous core of rewriteRelayUrl: given a URL, an optional cached port,
 * and an optional cached relay origin, return the rewritten URL.
 *
 * This mirrors the decision tree in the real rewriteRelayUrl without any
 * async Tauri calls, making it fully testable.
 */
function rewriteRelayUrlSync(url, cachedPort, cachedRelayOrigin) {
  const m = RELAY_MEDIA_RE.exec(url);
  if (!m) return url;

  if (cachedRelayOrigin && !isRelayMediaOrigin(url, cachedRelayOrigin)) {
    return url;
  }

  if (cachedPort && cachedPort > 0) {
    return `http://localhost:${cachedPort}/media/${m[1]}`;
  }

  return `sprout-media://localhost/media/${m[1]}`;
}

// A valid 64-char hex hash for testing
const HASH = "f7c93e1befa9a4e2aca30586ef10412fa7da3e5371e47376e710f6534433ea36";

// ── RELAY_MEDIA_RE ────────────────────────────────────────────────────

test("RELAY_MEDIA_RE: matches standard relay media URL", () => {
  assert.ok(
    RELAY_MEDIA_RE.test(`https://sprout.up.railway.app/media/${HASH}.png`),
  );
});

test("RELAY_MEDIA_RE: matches jpg, gif, webp, mp4 extensions", () => {
  for (const ext of ["jpg", "gif", "webp", "mp4"]) {
    assert.ok(
      RELAY_MEDIA_RE.test(`https://example.com/media/${HASH}.${ext}`),
      `should match .${ext}`,
    );
  }
});

test("RELAY_MEDIA_RE: matches thumbnail variant", () => {
  assert.ok(RELAY_MEDIA_RE.test(`https://example.com/media/${HASH}.thumb.jpg`));
});

test("RELAY_MEDIA_RE: matches URL with query string", () => {
  assert.ok(
    RELAY_MEDIA_RE.test(`https://example.com/media/${HASH}.png?w=400&h=300`),
  );
});

test("RELAY_MEDIA_RE: does not match non-media path", () => {
  assert.ok(!RELAY_MEDIA_RE.test("https://example.com/page"));
});

test("RELAY_MEDIA_RE: does not match hash shorter than 64 chars", () => {
  assert.ok(!RELAY_MEDIA_RE.test("https://example.com/media/abc123.png"));
});

test("RELAY_MEDIA_RE: does not match unsupported extension", () => {
  assert.ok(!RELAY_MEDIA_RE.test(`https://example.com/media/${HASH}.svg`));
});

test("RELAY_MEDIA_RE: does not match path without /media/ prefix", () => {
  assert.ok(!RELAY_MEDIA_RE.test(`https://example.com/files/${HASH}.png`));
});

// ── isRelayMediaOrigin ────────────────────────────────────────────────

test("isRelayMediaOrigin: exact origin match returns true", () => {
  const url = `https://sprout.up.railway.app/media/${HASH}.png`;
  assert.equal(isRelayMediaOrigin(url, "https://sprout.up.railway.app"), true);
});

test("isRelayMediaOrigin: same parent domain returns true (bug-fix scenario)", () => {
  // relay origin is sprout.up.railway.app but URL is from
  // sprout-relay-production.up.railway.app — same parent domain
  const url = `https://sprout-relay-production.up.railway.app/media/${HASH}.png`;
  assert.equal(isRelayMediaOrigin(url, "https://sprout.up.railway.app"), true);
});

test("isRelayMediaOrigin: completely different domain returns false", () => {
  const url = `https://nostr.build/media/${HASH}.png`;
  assert.equal(isRelayMediaOrigin(url, "https://sprout.up.railway.app"), false);
});

test("isRelayMediaOrigin: different TLD returns false", () => {
  const url = `https://sprout.up.railway.io/media/${HASH}.png`;
  assert.equal(isRelayMediaOrigin(url, "https://sprout.up.railway.app"), false);
});

test("isRelayMediaOrigin: subdomain of different parent returns false", () => {
  const url = `https://cdn.nostr.build/media/${HASH}.png`;
  assert.equal(isRelayMediaOrigin(url, "https://sprout.up.railway.app"), false);
});

test("isRelayMediaOrigin: same parent domain with deeper nesting returns true", () => {
  // Both share .stage.blox.sqprod.co
  const url = `https://sprout-media.stage.blox.sqprod.co/media/${HASH}.png`;
  assert.equal(
    isRelayMediaOrigin(url, "https://sprout-oss.stage.blox.sqprod.co"),
    true,
  );
});

test("isRelayMediaOrigin: two-part hostname does not match on parent domain", () => {
  // "railway.app" has only 2 parts — the guard (>= 3) prevents false positives
  const url = `https://railway.app/media/${HASH}.png`;
  assert.equal(isRelayMediaOrigin(url, "https://sprout.up.railway.app"), false);
});

test("isRelayMediaOrigin: malformed URL returns true (safe default)", () => {
  assert.equal(
    isRelayMediaOrigin("not-a-url", "https://sprout.up.railway.app"),
    true,
  );
});

// ── rewriteRelayUrlSync (full decision tree) ──────────────────────────

test("rewriteRelayUrl: non-media URL returned unchanged", () => {
  assert.equal(
    rewriteRelayUrlSync("https://example.com/page", null, null),
    "https://example.com/page",
  );
});

test("rewriteRelayUrl: external Blossom URL (different domain) returned unchanged", () => {
  // nostr.build is a real external Blossom host — not behind Cloudflare Access
  const url = `https://nostr.build/media/${HASH}.png`;
  assert.equal(
    rewriteRelayUrlSync(url, null, "https://sprout.up.railway.app"),
    url,
  );
});

test("rewriteRelayUrl: relay origin not cached → matching URL rewritten to sprout-media://", () => {
  // Safe default: when we don't know the relay origin yet, proxy everything
  // that looks like a relay media URL (avoids Cloudflare 403s)
  const url = `https://sprout.up.railway.app/media/${HASH}.png`;
  assert.equal(
    rewriteRelayUrlSync(url, null, null),
    `sprout-media://localhost/media/${HASH}.png`,
  );
});

test("rewriteRelayUrl: exact origin match → rewritten to sprout-media:// (no port cached)", () => {
  const url = `https://sprout.up.railway.app/media/${HASH}.png`;
  assert.equal(
    rewriteRelayUrlSync(url, null, "https://sprout.up.railway.app"),
    `sprout-media://localhost/media/${HASH}.png`,
  );
});

test("rewriteRelayUrl: exact origin match with port cached → rewritten to localhost proxy", () => {
  const url = `https://sprout.up.railway.app/media/${HASH}.png`;
  assert.equal(
    rewriteRelayUrlSync(url, 9876, "https://sprout.up.railway.app"),
    `http://localhost:9876/media/${HASH}.png`,
  );
});

test("rewriteRelayUrl: mismatched subdomain but same parent domain → rewritten (bug-fix)", () => {
  // This is the core bug-fix scenario: relay origin is sprout.up.railway.app
  // but SPROUT_MEDIA_BASE_URL points to sprout-relay-production.up.railway.app.
  // Without the parent-domain check, this URL would pass through unchanged
  // and get a Cloudflare 403 in WKWebView.
  const url = `https://sprout-relay-production.up.railway.app/media/${HASH}.png`;
  assert.equal(
    rewriteRelayUrlSync(url, 9876, "https://sprout.up.railway.app"),
    `http://localhost:9876/media/${HASH}.png`,
  );
});

test("rewriteRelayUrl: mismatched subdomain, no port cached → sprout-media:// fallback", () => {
  const url = `https://sprout-relay-production.up.railway.app/media/${HASH}.png`;
  assert.equal(
    rewriteRelayUrlSync(url, null, "https://sprout.up.railway.app"),
    `sprout-media://localhost/media/${HASH}.png`,
  );
});

test("rewriteRelayUrl: thumbnail variant is rewritten correctly", () => {
  const url = `https://sprout.up.railway.app/media/${HASH}.thumb.jpg`;
  assert.equal(
    rewriteRelayUrlSync(url, 9876, "https://sprout.up.railway.app"),
    `http://localhost:9876/media/${HASH}.thumb.jpg`,
  );
});

test("rewriteRelayUrl: query string is preserved through rewrite", () => {
  const url = `https://sprout.up.railway.app/media/${HASH}.png?w=400`;
  assert.equal(
    rewriteRelayUrlSync(url, 9876, "https://sprout.up.railway.app"),
    `http://localhost:9876/media/${HASH}.png?w=400`,
  );
});

test("rewriteRelayUrl: port 0 treated as uncached → sprout-media:// fallback", () => {
  // cachedPort = 0 means the proxy hasn't bound yet
  const url = `https://sprout.up.railway.app/media/${HASH}.png`;
  assert.equal(
    rewriteRelayUrlSync(url, 0, "https://sprout.up.railway.app"),
    `sprout-media://localhost/media/${HASH}.png`,
  );
});
