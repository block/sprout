import * as React from "react";

import { useManagedAgentsQuery } from "@/features/agents/hooks";
import { useChannelMembersQuery } from "@/features/channels/hooks";
import type { MentionSuggestion } from "@/features/messages/ui/MentionAutocomplete";

function detectMentionQuery(
  value: string,
  cursorPosition: number,
): { query: string; startIndex: number } | null {
  const beforeCursor = value.slice(0, cursorPosition);
  const match = beforeCursor.match(/(?:^|[\s])@([^\s]*)$/);
  if (!match) {
    return null;
  }

  const query = match[1];
  const startIndex = beforeCursor.length - query.length - 1; // -1 for @
  return { query, startIndex };
}

export function useMentions(channelId: string | null) {
  const [mentionQuery, setMentionQuery] = React.useState<string | null>(null);
  const [mentionStartIndex, setMentionStartIndex] = React.useState(0);
  const [mentionSelectedIndex, setMentionSelectedIndex] = React.useState(0);
  const mentionMapRef = React.useRef<Map<string, string>>(new Map());

  const membersQuery = useChannelMembersQuery(channelId);
  const members = membersQuery.data ?? [];
  const managedAgentsQuery = useManagedAgentsQuery();
  const managedAgentNamesByPubkey = React.useMemo(
    () =>
      new Map(
        (managedAgentsQuery.data ?? []).map((agent) => [
          agent.pubkey.toLowerCase(),
          agent.name,
        ]),
      ),
    [managedAgentsQuery.data],
  );

  const suggestions = React.useMemo<MentionSuggestion[]>(() => {
    if (mentionQuery === null) {
      return [];
    }

    const lowerQuery = mentionQuery.toLowerCase();
    return members
      .map((member) => {
        const fallbackName =
          managedAgentNamesByPubkey.get(member.pubkey.toLowerCase()) ??
          member.pubkey.slice(0, 8);

        return {
          member,
          label: member.displayName ?? fallbackName,
        };
      })
      .filter(
        ({ label, member }) =>
          label.toLowerCase().includes(lowerQuery) ||
          member.pubkey.toLowerCase().includes(lowerQuery),
      )
      .slice(0, 8)
      .map(({ member, label }) => ({
        pubkey: member.pubkey,
        displayName: label,
        role: member.role === "admin" ? "admin" : null,
      }));
  }, [managedAgentNamesByPubkey, members, mentionQuery]);

  const isMentionOpen = mentionQuery !== null && suggestions.length > 0;

  const insertMention = React.useCallback(
    (
      suggestion: MentionSuggestion,
      content: string,
      selectionEnd: number,
    ): { nextContent: string; nextCursor: number } => {
      const displayName = suggestion.displayName;
      const before = content.slice(0, mentionStartIndex);
      const after = content.slice(selectionEnd);
      const inserted = `@${displayName} `;
      const nextContent = `${before}${inserted}${after}`;
      const nextCursor = before.length + inserted.length;

      mentionMapRef.current.set(displayName, suggestion.pubkey);
      setMentionQuery(null);
      setMentionSelectedIndex(0);

      return { nextContent, nextCursor };
    },
    [mentionStartIndex],
  );

  const updateMentionQuery = React.useCallback(
    (value: string, cursorPosition: number) => {
      const mention = detectMentionQuery(value, cursorPosition);
      if (mention) {
        setMentionQuery(mention.query);
        setMentionStartIndex(mention.startIndex);
        setMentionSelectedIndex(0);
      } else {
        setMentionQuery(null);
      }
    },
    [],
  );

  const extractMentionPubkeys = React.useCallback(
    (text: string): string[] => {
      const pubkeys: string[] = [];

      const hasMention = (name: string): boolean => {
        const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
        const pattern = new RegExp(
          `(?:^|\\s)@${escaped}(?=[\\s,;.!?:)\\]}]|$)`,
          "i",
        );
        return pattern.test(text);
      };

      for (const [displayName, pubkey] of mentionMapRef.current) {
        if (hasMention(displayName)) {
          pubkeys.push(pubkey);
        }
      }

      for (const member of members) {
        if (pubkeys.includes(member.pubkey)) {
          continue;
        }
        const name =
          member.displayName ??
          managedAgentNamesByPubkey.get(member.pubkey.toLowerCase());
        if (name && hasMention(name)) {
          pubkeys.push(member.pubkey);
        }
      }

      return [...new Set(pubkeys)];
    },
    [members, managedAgentNamesByPubkey],
  );

  const clearMentions = React.useCallback(() => {
    mentionMapRef.current.clear();
    setMentionQuery(null);
    setMentionSelectedIndex(0);
  }, []);

  const handleMentionKeyDown = React.useCallback(
    (
      event: React.KeyboardEvent,
    ): { handled: boolean; suggestion?: MentionSuggestion } => {
      if (!isMentionOpen) {
        return { handled: false };
      }

      if (event.key === "ArrowDown") {
        event.preventDefault();
        setMentionSelectedIndex((current) =>
          current < suggestions.length - 1 ? current + 1 : 0,
        );
        return { handled: true };
      }

      if (event.key === "ArrowUp") {
        event.preventDefault();
        setMentionSelectedIndex((current) =>
          current > 0 ? current - 1 : suggestions.length - 1,
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
        return { handled: true, suggestion: suggestions[mentionSelectedIndex] };
      }

      if (event.key === "Escape") {
        event.preventDefault();
        setMentionQuery(null);
        return { handled: true };
      }

      return { handled: false };
    },
    [isMentionOpen, mentionSelectedIndex, suggestions],
  );

  return {
    clearMentions,
    extractMentionPubkeys,
    handleMentionKeyDown,
    insertMention,
    isMentionOpen,
    mentionSelectedIndex,
    suggestions,
    updateMentionQuery,
  };
}
