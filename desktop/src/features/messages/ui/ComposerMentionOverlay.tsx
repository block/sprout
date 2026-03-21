import type React from "react";

type Segment = { type: "text" | "mention" | "channel"; value: string };

const MENTION_OR_CHANNEL_RE = /@\S+|#[a-zA-Z0-9][\w-]*/g;

function parseSegments(text: string): Segment[] {
  const segments: Segment[] = [];
  let lastIndex = 0;

  for (const match of text.matchAll(MENTION_OR_CHANNEL_RE)) {
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
  scrollTop: number;
};

export function ComposerMentionOverlay({
  content,
  scrollTop,
}: ComposerMentionOverlayProps) {
  const segments = parseSegments(content);

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
