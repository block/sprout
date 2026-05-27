/**
 * Helpers for round-tripping NIP-92 imeta attachments through the message
 * editor.
 *
 * Background: edit events (kind 9 with `e` replace tag) carry only the new
 * `content`; imeta tags live on the original event. The renderer overlays the
 * edit body onto the original event but `markdown.tsx` only renders
 * <img>/<video> for URLs literally present in the body. So if the user edits
 * a message with an attachment, we must:
 *
 *   1. strip the trailing `![image|video](url)` markdown lines from the body
 *      before placing it in the editor (the user only sees/edits the text);
 *   2. re-append matching markdown lines for each imeta entry on submit, so
 *      the saved content keeps rendering the attachment.
 *
 * The list is mime-typed (`{ url, m }`) rather than bare URLs because
 * `markdown.tsx` distinguishes video by `.mp4` URL suffix today, and that's
 * brittle — passing the mime through lets the alt text stay correct
 * (`![video]` vs `![image]`) regardless of suffix.
 */

import { parseImetaTags } from "./parseImeta";

export type ImetaMedia = { url: string; m: string };

/**
 * Project a Nostr event's tags into the `{ url, m }` list the composer needs
 * for edit round-tripping. Order is preserved by `parseImetaTags`'s Map
 * insertion order, which matches tag order on the event.
 */
export function imetaMediaFromTags(
  tags: ReadonlyArray<ReadonlyArray<string>> | undefined,
): ImetaMedia[] {
  if (!tags || tags.length === 0) return [];
  const entries = parseImetaTags(tags as string[][]);
  const out: ImetaMedia[] = [];
  for (const entry of entries.values()) {
    if (entry.url && entry.m) out.push({ url: entry.url, m: entry.m });
  }
  return out;
}

const MEDIA_LINE_RE = /^!\[(?:image|video)\]\(([^)\s]+)\)\s*$/;

/**
 * Remove trailing `![image|video](url)` lines whose URL matches an entry in
 * `imetaMedia`. Stops at the first non-matching/non-blank line so attachments
 * that the user has manually moved or interleaved with text are left alone
 * (out-of-scope per task spec — we only handle the ordinary append-at-end
 * shape produced by the composer).
 */
export function stripImetaMediaLines(
  body: string,
  imetaMedia: ReadonlyArray<ImetaMedia>,
): string {
  if (imetaMedia.length === 0) return body;
  const urls = new Set(imetaMedia.map((m) => m.url));
  const lines = body.split("\n");

  // Walk from the end, peeling off blank lines and matching media lines.
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

/**
 * Append `\n![image|video](url)` for each imeta entry. Preserves entry order
 * (which mirrors tag order on the original event).
 */
export function appendImetaMediaLines(
  body: string,
  imetaMedia: ReadonlyArray<ImetaMedia>,
): string {
  if (imetaMedia.length === 0) return body;
  let out = body;
  for (const { url, m } of imetaMedia) {
    const isVideo = m.startsWith("video/");
    out += isVideo ? `\n![video](${url})` : `\n![image](${url})`;
  }
  return out;
}
