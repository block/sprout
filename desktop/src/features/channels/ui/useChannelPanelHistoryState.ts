import * as React from "react";
import { useNavigate, useSearch } from "@tanstack/react-router";

/**
 * Auxiliary-panel state for the channel routes, backed by URL search params
 * so it lives in the history stack: back/forward restores the panel a given
 * entry was showing, and reloads restore the panel from the URL.
 *
 * Params: `thread` (open thread head id), `profile` (profile panel pubkey),
 * `agentSession` (agent session panel pubkey).
 *
 * Setter calls made synchronously within one event handler are coalesced
 * into a single navigation, so each user action produces exactly one
 * history entry even when a handler closes one panel and opens another.
 */

export type PanelSetterOptions = {
  /** Rewrite the current entry instead of pushing a new one. */
  replace?: boolean;
};

export type PanelValueSetter = (
  value: string | null,
  options?: PanelSetterOptions,
) => void;

type ChannelPanelSearch = {
  agentSession?: string;
  profile?: string;
  thread?: string;
};

type PanelPatch = {
  agentSession?: string | null;
  profile?: string | null;
  thread?: string | null;
};

const PANEL_SEARCH_KEYS = ["agentSession", "profile", "thread"] as const;

export function useChannelPanelHistoryState() {
  const navigate = useNavigate();
  const search = useSearch({ strict: false }) as ChannelPanelSearch;

  const openThreadHeadId = search.thread ?? null;
  const profilePanelPubkey = search.profile ?? null;
  const openAgentSessionPubkey = search.agentSession ?? null;

  const currentValuesRef = React.useRef<Required<PanelPatch>>({
    agentSession: openAgentSessionPubkey,
    profile: profilePanelPubkey,
    thread: openThreadHeadId,
  });
  currentValuesRef.current = {
    agentSession: openAgentSessionPubkey,
    profile: profilePanelPubkey,
    thread: openThreadHeadId,
  };

  const pendingRef = React.useRef<{
    patch: PanelPatch;
    replace: boolean;
  } | null>(null);

  const applyPanelPatch = React.useCallback(
    (patch: PanelPatch, options?: PanelSetterOptions) => {
      const pending = pendingRef.current;
      if (pending) {
        Object.assign(pending.patch, patch);
        pending.replace = pending.replace || Boolean(options?.replace);
        return;
      }

      pendingRef.current = {
        patch: { ...patch },
        replace: Boolean(options?.replace),
      };
      queueMicrotask(() => {
        const flush = pendingRef.current;
        pendingRef.current = null;
        if (!flush) {
          return;
        }

        const currentValues = currentValuesRef.current;
        const isChanged = PANEL_SEARCH_KEYS.some(
          (key) =>
            flush.patch[key] !== undefined &&
            (flush.patch[key] ?? null) !== currentValues[key],
        );
        if (!isChanged) {
          return;
        }

        void navigate({
          to: ".",
          search: (previousSearch: Record<string, unknown>) => {
            const nextSearch = { ...previousSearch };
            for (const key of PANEL_SEARCH_KEYS) {
              const value = flush.patch[key];
              if (value === undefined) {
                continue;
              }
              if (value === null) {
                delete nextSearch[key];
              } else {
                nextSearch[key] = value;
              }
            }
            return nextSearch;
          },
          replace: flush.replace,
          resetScroll: false,
        } as never);
      });
    },
    [navigate],
  );

  const setOpenThreadHeadId = React.useCallback<PanelValueSetter>(
    (value, options) => applyPanelPatch({ thread: value }, options),
    [applyPanelPatch],
  );

  const setProfilePanelPubkey = React.useCallback<PanelValueSetter>(
    (value, options) => applyPanelPatch({ profile: value }, options),
    [applyPanelPatch],
  );

  const setOpenAgentSessionPubkey = React.useCallback<PanelValueSetter>(
    (value, options) => applyPanelPatch({ agentSession: value }, options),
    [applyPanelPatch],
  );

  return {
    openAgentSessionPubkey,
    openThreadHeadId,
    profilePanelPubkey,
    setOpenAgentSessionPubkey,
    setOpenThreadHeadId,
    setProfilePanelPubkey,
  };
}
