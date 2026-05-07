/**
 * Rewrite relay media URLs to use the localhost streaming proxy.
 *
 * WKWebView's networking stack bypasses WARP, so direct <img src> requests
 * to the relay get 403'd by Cloudflare Access. The localhost proxy routes
 * fetches through the Rust backend (via reqwest), which goes through WARP.
 *
 * For video, the proxy streams via axum — no buffering the entire file.
 * Images and other media also benefit from this path.
 *
 * Only URLs hosted on the Sprout relay are rewritten. External Blossom URLs
 * (e.g. nostr.build, void.cat) are returned unchanged — they aren't behind
 * Cloudflare Access and can be loaded directly by WKWebView. Without this
 * origin check, external Blossom URLs would be proxied to the wrong server
 * (the Sprout relay), resulting in 404s.
 */

import { invoke } from "@tauri-apps/api/core";

// Matches: https://anything.com/media/{64-hex}.{ext}
// Also matches thumbnails: /media/{64-hex}.thumb.jpg
const RELAY_MEDIA_RE =
  /^(?:https?:\/\/[^/]+)\/media\/([\da-f]{64}(?:\.thumb)?\.(?:jpg|png|gif|webp|mp4)(?:\?.*)?)$/;

/** Cached proxy port — fetched once from the Tauri backend. */
let cachedPort: number | null = null;
let portPromise: Promise<number | null> | null = null;

/** Cached relay origin (e.g. "https://sprout-oss.stage.blox.sqprod.co"). */
let cachedRelayOrigin: string | null = null;

const POLL_INTERVAL_MS = 100;
const POLL_TIMEOUT_MS = 5000;

/**
 * Poll `get_media_proxy_port` until we get a non-zero port or timeout.
 * Also fetches the relay HTTP base URL for origin-checking.
 * Returns the port, or null if the proxy never came up.
 */
async function fetchProxyPort(): Promise<number | null> {
  // Fetch relay origin in parallel — fire-and-forget, no retry needed.
  if (!cachedRelayOrigin) {
    invoke<string>("get_relay_http_url")
      .then((url) => {
        cachedRelayOrigin = url.replace(/\/+$/, "");
      })
      .catch(() => {});
  }

  const deadline = Date.now() + POLL_TIMEOUT_MS;
  while (Date.now() < deadline) {
    try {
      const port = await invoke<number>("get_media_proxy_port");
      if (port > 0) {
        cachedPort = port;
        return port;
      }
    } catch {
      // invoke failed (e.g. Tauri IPC not ready yet) — keep retrying
    }
    await new Promise((r) => setTimeout(r, POLL_INTERVAL_MS));
  }
  return null;
}

/** Eagerly fetch the port at module load so it's ready by first render. */
// The try/catch inside fetchProxyPort handles non-Tauri environments gracefully
// (invoke will throw, we retry until timeout, then give up — no side effects).
if (typeof window !== "undefined") {
  portPromise = fetchProxyPort();
}

/**
 * Reset module-level caches so the next render re-fetches the proxy port
 * and relay origin for the new workspace.
 */
export function resetMediaCaches(): void {
  cachedPort = null;
  portPromise = null;
  cachedRelayOrigin = null;
}

/**
 * If `url` is a Blossom media URL hosted on the Sprout relay, rewrite it
 * to go through the localhost streaming proxy. External Blossom URLs and
 * non-Blossom URLs are returned unchanged.
 *
 * Falls back to sprout-media:// if the proxy port isn't available yet.
 */
export function rewriteRelayUrl(url: string): string {
  const m = RELAY_MEDIA_RE.exec(url);
  if (!m) return url;

  // Only proxy URLs that belong to our relay. External Blossom URLs
  // (different origin) pass through unchanged — they work fine via WKWebView.
  // If the relay origin isn't cached yet, fall through to the rewrite path
  // as a safe default (relay URLs need the proxy to avoid Cloudflare 403s).
  //
  // We compare by extracting the origin from the URL and checking against
  // the relay origin. We also accept URLs whose origin matches the relay's
  // hostname with any subdomain prefix (handles cases where the relay's
  // SPROUT_MEDIA_BASE_URL is stale/mismatched with the actual relay URL).
  if (cachedRelayOrigin && !isRelayMediaOrigin(url, cachedRelayOrigin)) {
    return url;
  }

  if (cachedPort && cachedPort > 0) {
    return `http://localhost:${cachedPort}/media/${m[1]}`;
  }

  // Kick off fetch if we haven't yet.
  if (!portPromise && typeof window !== "undefined") {
    portPromise = fetchProxyPort();
  }

  return `sprout-media://localhost/media/${m[1]}`;
}

/**
 * Check if a media URL belongs to our relay. Matches if:
 * 1. The URL starts with the relay origin (exact match), OR
 * 2. The URL's hostname shares the same base domain as the relay
 *    (e.g. "sprout-relay-production.up.railway.app" matches relay origin
 *    "https://sprout.up.railway.app" because both are *.up.railway.app).
 *
 * This handles the case where the relay's SPROUT_MEDIA_BASE_URL env var
 * points to a different hostname than the relay's actual public URL.
 */
function isRelayMediaOrigin(url: string, relayOrigin: string): boolean {
  if (url.startsWith(`${relayOrigin}/`)) {
    return true;
  }

  // Extract hostnames and compare base domains.
  // "https://foo.up.railway.app/media/..." → "foo.up.railway.app"
  try {
    const urlHost = new URL(url).hostname;
    const relayHost = new URL(relayOrigin).hostname;

    // Exact match
    if (urlHost === relayHost) return true;

    // Share the same parent domain (≥2 levels).
    // e.g. both end in ".up.railway.app" or ".stage.blox.sqprod.co"
    const urlParts = urlHost.split(".");
    const relayParts = relayHost.split(".");

    if (urlParts.length >= 3 && relayParts.length >= 3) {
      // Compare last N-1 parts (the parent domain)
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
