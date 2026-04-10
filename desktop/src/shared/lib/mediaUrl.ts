/**
 * Rewrite relay media URLs to use the sprout-media:// custom protocol.
 *
 * WKWebView's networking stack bypasses WARP, so direct <img src> requests
 * to the relay get 403'd by Cloudflare Access. The sprout-media:// scheme
 * routes fetches through the Rust backend, which goes through WARP.
 *
 * Detection is path-based: /media/{64-hex-chars}.{ext} is a Blossom BUD-01
 * content-addressed URL. The 64-char lowercase hex SHA-256 hash makes this
 * pattern unique to Blossom relays — false positives from other origins are
 * practically impossible. This avoids needing async relay-URL initialization,
 * eliminating race conditions with first render.
 */

// Matches: https://anything.com/media/{64-hex}.{ext}
// Also matches thumbnails: /media/{64-hex}.thumb.jpg
const RELAY_MEDIA_RE =
  /^(?:https?:\/\/[^/]+)\/media\/([\da-f]{64}(?:\.thumb)?\.(?:jpg|png|gif|webp|mp4)(?:\?.*)?)$/;

/**
 * If `url` looks like a Blossom relay media URL, rewrite it to go through
 * the sprout-media:// custom protocol. Otherwise return it unchanged.
 */
export function rewriteRelayUrl(url: string): string {
  const m = RELAY_MEDIA_RE.exec(url);
  if (!m) return url;
  return `sprout-media://localhost/media/${m[1]}`;
}
