import React from "react";

import { buildMentionPattern } from "@/shared/lib/mentionPattern";

type Segment = { type: "text" | "mention" | "channel"; value: string };

const CHANNEL_RE_PART = "#[a-zA-Z0-9][\\w-]*";

/**
 * Extends the shared mention pattern to also match `#channel-name` references.
 */
function buildOverlayPattern(mentionNames: string[]): RegExp {
  const mentionSource = buildMentionPattern(mentionNames).source;
  return new RegExp(`${mentionSource}|${CHANNEL_RE_PART}`, "g");
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
  content: string;
  mentionNames: string[];
  scrollTop: number;
};

export function ComposerMentionOverlay({
  content,
  mentionNames,
  scrollTop,
}: ComposerMentionOverlayProps) {
  const pattern = React.useMemo(
    () => buildOverlayPattern(mentionNames),
    [mentionNames],
  );
  const segments = parseSegments(content, pattern);

  return (
    <div
      className="whitespace-pre-wrap break-words px-0 py-0 text-sm leading-6"
      style={{ transform: `translateY(-${scrollTop}px)` }}
    >
      {
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
        ).nodes
      }
    </div>
  );
}
