import * as React from "react";

import { useChannelNavigation } from "@/shared/context/ChannelNavigationContext";

export type ChannelSuggestion = {
  id: string;
  name: string;
  channelType: "stream" | "forum";
};

function detectChannelQuery(
  value: string,
  cursorPosition: number,
  knownNamesLower: string[],
): { query: string; startIndex: number } | null {
  const beforeCursor = value.slice(0, cursorPosition);

  // Fast path: single-word channel query (no spaces after #)
  const simpleMatch = beforeCursor.match(/(?:^|[\s])#([^\s]*)$/);
  if (simpleMatch) {
    const query = simpleMatch[1];
    const startIndex = beforeCursor.length - query.length - 1; // -1 for #
    return { query, startIndex };
  }

  // Multi-word path: scan backwards for a `#` and check if the text between
  // `#` and the cursor is a prefix of any known multi-word channel name.
  const scanStart = Math.max(0, beforeCursor.length - 80);
  for (let i = beforeCursor.length - 1; i >= scanStart; i--) {
    const ch = beforeCursor[i];
    if (ch === "#") {
      // Ensure `#` is at start or preceded by whitespace
      if (i > 0 && !/\s/.test(beforeCursor[i - 1])) {
        continue;
      }
      const candidate = beforeCursor.slice(i + 1);
      if (candidate.length === 0) {
        break;
      }
      const lowerCandidate = candidate.toLowerCase();
      const isPrefix = knownNamesLower.some((name) =>
        name.startsWith(lowerCandidate),
      );
      if (isPrefix) {
        return { query: candidate, startIndex: i };
      }
      break;
    }
    // Stop scanning if we hit a newline
    if (ch === "\n") {
      break;
    }
  }

  return null;
}

const CHANNEL_QUERY_DEBOUNCE_MS = 120;

export function useChannelLinks() {
  const { channels } = useChannelNavigation();

  const [channelQuery, setChannelQuery] = React.useState<string | null>(null);
  const [channelStartIndex, setChannelStartIndex] = React.useState(0);
  const [channelSelectedIndex, setChannelSelectedIndex] = React.useState(0);

  const debounceTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const latestValueRef = React.useRef<string>("");
  const latestCursorRef = React.useRef<number>(0);

  /** Lower-cased channel names for case-insensitive prefix matching. */
  const knownNamesLower = React.useMemo<string[]>(
    () =>
      channels
        .filter((ch) => ch.channelType !== "dm")
        .map((ch) => ch.name.toLowerCase()),
    [channels],
  );

  const knownNamesLowerRef = React.useRef<string[]>(knownNamesLower);

  // Keep the known-names ref in sync so the debounced callback never reads stale data.
  React.useEffect(() => {
    knownNamesLowerRef.current = knownNamesLower;
  }, [knownNamesLower]);

  // Clean up pending timeout on unmount
  React.useEffect(() => {
    return () => {
      if (debounceTimerRef.current !== null) {
        clearTimeout(debounceTimerRef.current);
      }
    };
  }, []);

  const channelSuggestions = React.useMemo<ChannelSuggestion[]>(() => {
    if (channelQuery === null) {
      return [];
    }

    const lowerQuery = channelQuery.toLowerCase();
    return channels
      .filter(
        (ch) =>
          ch.channelType !== "dm" && ch.name.toLowerCase().includes(lowerQuery),
      )
      .slice(0, 8)
      .map((ch) => ({
        id: ch.id,
        name: ch.name,
        channelType: ch.channelType as "stream" | "forum",
      }));
  }, [channels, channelQuery]);

  const isChannelOpen = channelQuery !== null && channelSuggestions.length > 0;

  const insertChannel = React.useCallback(
    (
      suggestion: ChannelSuggestion,
      content: string,
      selectionEnd: number,
    ): { nextContent: string; nextCursor: number } => {
      // Cancel any pending debounced detection — user already selected
      if (debounceTimerRef.current !== null) {
        clearTimeout(debounceTimerRef.current);
        debounceTimerRef.current = null;
      }

      const before = content.slice(0, channelStartIndex);
      const after = content.slice(selectionEnd);
      const inserted = `#${suggestion.name} `;
      const nextContent = `${before}${inserted}${after}`;
      const nextCursor = before.length + inserted.length;

      setChannelQuery(null);
      setChannelSelectedIndex(0);

      return { nextContent, nextCursor };
    },
    [channelStartIndex],
  );

  const updateChannelQuery = React.useCallback(
    (value: string, cursorPosition: number) => {
      // Store latest values so the debounced callback always uses fresh data
      latestValueRef.current = value;
      latestCursorRef.current = cursorPosition;

      if (debounceTimerRef.current !== null) {
        clearTimeout(debounceTimerRef.current);
      }

      debounceTimerRef.current = setTimeout(() => {
        debounceTimerRef.current = null;
        const channel = detectChannelQuery(
          latestValueRef.current,
          latestCursorRef.current,
          knownNamesLowerRef.current,
        );
        if (channel) {
          setChannelQuery(channel.query);
          setChannelStartIndex(channel.startIndex);
          setChannelSelectedIndex(0);
        } else {
          setChannelQuery(null);
        }
      }, CHANNEL_QUERY_DEBOUNCE_MS);
    },
    [],
  );

  const clearChannels = React.useCallback(() => {
    if (debounceTimerRef.current !== null) {
      clearTimeout(debounceTimerRef.current);
      debounceTimerRef.current = null;
    }
    setChannelQuery(null);
    setChannelSelectedIndex(0);
  }, []);

  const handleChannelKeyDown = React.useCallback(
    (
      event: React.KeyboardEvent,
    ): { handled: boolean; suggestion?: ChannelSuggestion } => {
      if (!isChannelOpen) {
        return { handled: false };
      }

      if (event.key === "ArrowDown") {
        event.preventDefault();
        setChannelSelectedIndex((current) =>
          current < channelSuggestions.length - 1 ? current + 1 : 0,
        );
        return { handled: true };
      }

      if (event.key === "ArrowUp") {
        event.preventDefault();
        setChannelSelectedIndex((current) =>
          current > 0 ? current - 1 : channelSuggestions.length - 1,
        );
        return { handled: true };
      }

      if (
        event.key === "Tab" ||
        (event.key === "Enter" &&
          !event.ctrlKey &&
          !event.metaKey &&
          !event.altKey &&
          !event.shiftKey)
      ) {
        event.preventDefault();
        return {
          handled: true,
          suggestion: channelSuggestions[channelSelectedIndex],
        };
      }

      if (event.key === "Escape") {
        event.preventDefault();
        setChannelQuery(null);
        return { handled: true };
      }

      return { handled: false };
    },
    [isChannelOpen, channelSelectedIndex, channelSuggestions],
  );

  return {
    channelQuery,
    channelSelectedIndex,
    channelSuggestions,
    clearChannels,
    handleChannelKeyDown,
    insertChannel,
    isChannelOpen,
    updateChannelQuery,
  };
}
