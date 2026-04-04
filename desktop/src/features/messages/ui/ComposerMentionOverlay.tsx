import React from "react";

import { buildPrefixPattern } from "@/shared/lib/mentionPattern";

type Segment = { type: "text" | "mention" | "channel"; value: string };

/**
 * Extends the shared mention pattern to also match `#channel-name` references.
 */
function buildOverlayPattern(
  mentionNames: string[],
  channelNames: string[],
): RegExp {
  const mentionSource = buildPrefixPattern("@", mentionNames).source;
  const channelSource = buildPrefixPattern("#", channelNames).source;
  return new RegExp(`${mentionSource}|${channelSource}`, "gi");
}

function parseSegments(text: string, pattern: RegExp): Segment[] {
  const segments: Segment[] = [];
  let lastIndex = 0;

  pattern.lastIndex = 0;
  for (const match of text.matchAll(pattern)) {
    const matchStart = match.index;
    if (matchStart > lastIndex) {
      segments.push({ type: "text", value: text.slice(lastIndex, matchStart) });
    }

    const value = match[0];
    segments.push({
      type: value.startsWith("@") ? "mention" : "channel",
      value,
    });
    lastIndex = matchStart + value.length;
  }

  if (lastIndex < text.length) {
    segments.push({ type: "text", value: text.slice(lastIndex) });
  }

  return segments;
}

type ComposerMentionOverlayProps = {
  channelNames: string[];
  content: string;
  mentionNames: string[];
  scrollTop: number;
};

/**
 * Overlay that highlights @mentions and #channels in the composer text.
 *
 * Wrapped in React.memo so it skips re-renders when the parent re-renders
 * without changing any of this component's props (e.g. focus state changes).
 */
export const ComposerMentionOverlay = React.memo(
  function ComposerMentionOverlay({
    channelNames,
    content,
    mentionNames,
    scrollTop,
  }: ComposerMentionOverlayProps) {
    const pattern = React.useMemo(
      () => buildOverlayPattern(mentionNames, channelNames),
      [mentionNames, channelNames],
    );
    // Memoize regex parsing so it only re-runs when content or the mention
    // pattern actually changes — avoids expensive matchAll on every render.
    const segments = React.useMemo(
      () => parseSegments(content, pattern),
      [content, pattern],
    );

    // Memoize the rendered nodes so we don't rebuild the element array unless
    // the parsed segments change.
    const renderedNodes = React.useMemo(
      () =>
        segments.reduce<{ offset: number; nodes: React.ReactNode[] }>(
          (acc, segment) => {
            const key = `${acc.offset}`;
            if (segment.type === "mention" || segment.type === "channel") {
              acc.nodes.push(
                <span
                  className="rounded-sm bg-primary/15 text-primary"
                  key={key}
                >
                  {segment.value}
                </span>,
              );
            } else {
              acc.nodes.push(
                <span className="text-foreground" key={key}>
                  {segment.value}
                </span>,
              );
            }
            acc.offset += segment.value.length;
            return acc;
          },
          { offset: 0, nodes: [] },
        ).nodes,
      [segments],
    );

    return (
      <div
        className="whitespace-pre-wrap break-words px-0 py-0 text-sm leading-6"
        style={{ transform: `translateY(-${scrollTop}px)` }}
      >
        {renderedNodes}
      </div>
    );
  },
);
