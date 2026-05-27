/**
 * Remark plugin that detects bare `sprout://message?…` URLs in text nodes and
 * replaces each with a custom `message-link` HAST element. The `markdown.tsx`
 * components map renders that as an inline pill (channel name + click-to-open)
 * instead of the raw 100-char URL.
 *
 * Why this plugin exists: `remark-gfm`'s autolinker only covers `http(s)://`
 * and `www.`. Custom schemes like `sprout://` only reach the `<a>` component
 * override when the user wrote an explicit `[label](sprout://…)` link.
 *
 * Mirrors `remarkChannelLinks` / `remarkMentions` — same factory, same HAST
 * shape — so the rendering layer treats all three uniformly.
 */
// Explicit `.ts` extension lets this plugin be imported both by the Vite-built
// `markdown.tsx` and by `markdown.test.mjs` running under `node --test
// --experimental-strip-types`. `tsconfig.json` enables `allowImportingTsExtensions`.
import { createRemarkPrefixPlugin } from "../../../shared/lib/createRemarkPrefixPlugin.ts";

const MESSAGE_URL_PATTERN = /sprout:\/\/message\?[^\s<>"')\]]+/g;

export default function remarkMessageLinks() {
  return createRemarkPrefixPlugin(MESSAGE_URL_PATTERN, (matchText) => ({
    type: "message-link",
    value: matchText,
    data: {
      hName: "message-link",
      hChildren: [{ type: "text", value: matchText }],
    },
  }));
}
