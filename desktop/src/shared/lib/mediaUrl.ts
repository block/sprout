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
 * Detection is path-based: /media/{64-hex-chars}.{ext} is a Blossom BUD-01
 * content-addressed URL. The 64-char lowercase hex SHA-256 hash makes this
 * pattern unique to Blossom relays — false positives from other origins are
 * practically impossible. This avoids needing async relay-URL initialization,
 * eliminating race conditions with first render.
 */

import { invoke } from "@tauri-apps/api/core";

// Matches: https://anything.com/media/{64-hex}.{ext}
// Also matches thumbnails: /media/{64-hex}.thumb.jpg
const RELAY_MEDIA_RE =
  /^(?:https?:\/\/[^/]+)\/media\/([\da-f]{64}(?:\.thumb)?\.(?:jpg|png|gif|webp|mp4)(?:\?.*)?)$/;

/** Cached proxy port — fetched once from the Tauri backend. */
let cachedPort: number | null = null;
let portPromise: Promise<number | null> | null = null;

const POLL_INTERVAL_MS = 100;
const POLL_TIMEOUT_MS = 5000;

/**
 * Poll `get_media_proxy_port` until we get a non-zero port or timeout.
 * Returns the port, or null if the proxy never came up.
 */
async function fetchProxyPort(): Promise<number | null> {
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
 * If `url` looks like a Blossom relay media URL, rewrite it to go through
 * the localhost streaming proxy. Falls back to sprout-media:// if the proxy
 * port isn't available yet.
 */
export function rewriteRelayUrl(url: string): string {
  const m = RELAY_MEDIA_RE.exec(url);
  if (!m) return url;

  if (cachedPort && cachedPort > 0) {
    return `http://localhost:${cachedPort}/media/${m[1]}`;
  }

  // Kick off fetch if we haven't yet.
  if (!portPromise && typeof window !== "undefined") {
    portPromise = fetchProxyPort();
  }

  return `sprout-media://localhost/media/${m[1]}`;
}
