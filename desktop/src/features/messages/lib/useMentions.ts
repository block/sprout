import * as React from "react";

import { useManagedAgentsQuery } from "@/features/agents/hooks";
import { useChannelMembersQuery } from "@/features/channels/hooks";
import type { MentionSuggestion } from "@/features/messages/ui/MentionAutocomplete";
import { detectPrefixQuery } from "@/shared/lib/detectPrefixQuery";
import { escapeRegExp } from "@/shared/lib/mentionPattern";

const MENTION_DEBOUNCE_MS = 120;

export function useMentions(channelId: string | null) {
  const [mentionQuery, setMentionQuery] = React.useState<string | null>(null);
  const [mentionStartIndex, setMentionStartIndex] = React.useState(0);
  const [mentionSelectedIndex, setMentionSelectedIndex] = React.useState(0);
  const mentionMapRef = React.useRef<Map<string, string>>(new Map());

  const membersQuery = useChannelMembersQuery(channelId);
  const members = membersQuery.data;
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

  const knownNames = React.useMemo<string[]>(() => {
    if (!members) return [];
    const names: string[] = [];
    for (const member of members) {
      const name =
        member.displayName ??
        managedAgentNamesByPubkey.get(member.pubkey.toLowerCase());
      if (name) {
        names.push(name);
      }
    }
    return names;
  }, [members, managedAgentNamesByPubkey]);

  /** Lower-cased version of knownNames, used for case-insensitive prefix matching. */
  const knownNamesLower = React.useMemo<string[]>(
    () => knownNames.map((n) => n.toLowerCase()),
    [knownNames],
  );

  // --- Debounce infrastructure for updateMentionQuery ---
  const debounceTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const latestValueRef = React.useRef<string>("");
  const latestCursorRef = React.useRef<number>(0);
  const knownNamesLowerRef = React.useRef<string[]>(knownNamesLower);

  // Keep the known-names ref in sync so the debounced callback never reads stale data.
  React.useEffect(() => {
    knownNamesLowerRef.current = knownNamesLower;
  }, [knownNamesLower]);

  // Clean up any pending debounce timer on unmount.
  React.useEffect(() => {
    return () => {
      if (debounceTimerRef.current !== null) {
        clearTimeout(debounceTimerRef.current);
      }
    };
  }, []);

  const suggestions = React.useMemo<MentionSuggestion[]>(() => {
    if (mentionQuery === null) {
      return [];
    }

    const lowerQuery = mentionQuery.toLowerCase();
    return (members ?? [])
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
      // Cancel any pending debounced detection — user already selected
      if (debounceTimerRef.current !== null) {
        clearTimeout(debounceTimerRef.current);
        debounceTimerRef.current = null;
      }

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
      // Stash the latest values so the debounced callback always uses fresh data.
      latestValueRef.current = value;
      latestCursorRef.current = cursorPosition;

      // Clear any previously scheduled detection.
      if (debounceTimerRef.current !== null) {
        clearTimeout(debounceTimerRef.current);
      }

      debounceTimerRef.current = setTimeout(() => {
        debounceTimerRef.current = null;

        const mention = detectPrefixQuery(
          "@",
          latestValueRef.current,
          latestCursorRef.current,
          knownNamesLowerRef.current,
        );
        if (mention) {
          setMentionQuery(mention.query);
          setMentionStartIndex(mention.startIndex);
          setMentionSelectedIndex(0);
        } else {
          setMentionQuery(null);
        }
      }, MENTION_DEBOUNCE_MS);
    },
    // Stable: refs are used inside the timeout, so no reactive deps needed.
    [],
  );

  const extractMentionPubkeys = React.useCallback(
    (text: string): string[] => {
      const pubkeys: string[] = [];

      const hasMention = (name: string): boolean => {
        const escaped = escapeRegExp(name);
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

      for (const member of members ?? []) {
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
    if (debounceTimerRef.current !== null) {
      clearTimeout(debounceTimerRef.current);
      debounceTimerRef.current = null;
    }
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
    knownNames,
    mentionSelectedIndex,
    suggestions,
    updateMentionQuery,
  };
}
