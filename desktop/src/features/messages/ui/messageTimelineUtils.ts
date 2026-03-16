import type { TimelineReaction } from "@/features/messages/types";

const BOTTOM_THRESHOLD_PX = 72;
export const DEFAULT_EMOJI_OPTIONS = [
  "👍",
  "❤️",
  "🎉",
  "🚀",
  "👀",
  "✅",
  "🔥",
  "👎",
];

export function isNearBottom(container: HTMLDivElement) {
  return (
    container.scrollHeight - container.clientHeight - container.scrollTop <=
    BOTTOM_THRESHOLD_PX
  );
}

export function getReactionOptions(reactions: TimelineReaction[]) {
  const seen = new Set<string>();
  const options: string[] = [];

  for (const reaction of reactions) {
    if (seen.has(reaction.emoji)) {
      continue;
    }

    seen.add(reaction.emoji);
    options.push(reaction.emoji);
  }

  for (const emoji of DEFAULT_EMOJI_OPTIONS) {
    if (seen.has(emoji)) {
      continue;
    }

    seen.add(emoji);
    options.push(emoji);
  }

  return options;
}
