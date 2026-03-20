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
): { query: string; startIndex: number } | null {
  const beforeCursor = value.slice(0, cursorPosition);
  const match = beforeCursor.match(/(?:^|[\s])#([^\s]*)$/);
  if (!match) {
    return null;
  }

  const query = match[1];
  const startIndex = beforeCursor.length - query.length - 1; // -1 for #
  return { query, startIndex };
}

export function useChannelLinks() {
  const { channels } = useChannelNavigation();

  const [channelQuery, setChannelQuery] = React.useState<string | null>(null);
  const [channelStartIndex, setChannelStartIndex] = React.useState(0);
  const [channelSelectedIndex, setChannelSelectedIndex] = React.useState(0);

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
      const channel = detectChannelQuery(value, cursorPosition);
      if (channel) {
        setChannelQuery(channel.query);
        setChannelStartIndex(channel.startIndex);
        setChannelSelectedIndex(0);
      } else {
        setChannelQuery(null);
      }
    },
    [],
  );

  const clearChannels = React.useCallback(() => {
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
