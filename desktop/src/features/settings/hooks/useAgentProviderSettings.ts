import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";

import {
  type AgentProviderProfileLoadStatus,
  type AgentProviderSettingsInput,
  type AgentProviderSettingsState,
  deleteAgentProviderProfile,
  deleteAgentProviderSettings,
  getAgentProviderEnvPresence,
  getAgentProviderProfile,
  getAgentProviderSettingsState,
  saveAgentProviderProfile,
  setDefaultAgentProviderProfile,
} from "@/features/settings/lib/agentProviderSettingsApi.ts";

const AGENT_PROVIDER_SETTINGS_STATE_KEY = [
  "agent-provider-settings-state",
] as const;
const AGENT_PROVIDER_PROFILE_KEY = ["agent-provider-profile"] as const;
const AGENT_PROVIDER_ENV_KEY = ["agent-provider-env-presence"] as const;

/** List of all profiles + default-id (or none/identity-mismatch/error). */
export function useAgentProviderSettingsStateQuery() {
  return useQuery<AgentProviderSettingsState>({
    queryKey: AGENT_PROVIDER_SETTINGS_STATE_KEY,
    queryFn: getAgentProviderSettingsState,
    // Local file — cache forever within a session.
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: Number.POSITIVE_INFINITY,
  });
}

/**
 * Single-profile editable view. `profileId === null` returns a no-op query
 * (used by the create dialog so we can keep hook order stable).
 */
export function useAgentProviderProfileQuery(profileId: string | null) {
  return useQuery<AgentProviderProfileLoadStatus>({
    queryKey: [...AGENT_PROVIDER_PROFILE_KEY, profileId ?? "__none__"],
    queryFn: () => getAgentProviderProfile(profileId as string),
    enabled: profileId !== null,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: Number.POSITIVE_INFINITY,
  });
}

export function useAgentProviderEnvPresenceQuery() {
  return useQuery({
    queryKey: AGENT_PROVIDER_ENV_KEY,
    queryFn: getAgentProviderEnvPresence,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: Number.POSITIVE_INFINITY,
  });
}

/** Invalidate both the list state and any cached single-profile view. */
function useInvalidateAll() {
  const queryClient = useQueryClient();
  return () => {
    void queryClient.invalidateQueries({
      queryKey: AGENT_PROVIDER_SETTINGS_STATE_KEY,
    });
    void queryClient.invalidateQueries({
      queryKey: AGENT_PROVIDER_PROFILE_KEY,
    });
  };
}

export function useSaveAgentProviderProfileMutation() {
  const invalidateAll = useInvalidateAll();
  return useMutation({
    mutationFn: (input: AgentProviderSettingsInput) =>
      saveAgentProviderProfile(input),
    onSuccess: () => invalidateAll(),
  });
}

export function useSetDefaultAgentProviderProfileMutation() {
  const invalidateAll = useInvalidateAll();
  return useMutation({
    mutationFn: (profileId: string | null) =>
      setDefaultAgentProviderProfile(profileId),
    onSuccess: () => invalidateAll(),
  });
}

export function useDeleteAgentProviderProfileMutation() {
  const invalidateAll = useInvalidateAll();
  return useMutation({
    mutationFn: (profileId: string) => deleteAgentProviderProfile(profileId),
    onSuccess: () => invalidateAll(),
  });
}

export function useDeleteAgentProviderSettingsMutation() {
  const invalidateAll = useInvalidateAll();
  return useMutation({
    mutationFn: () => deleteAgentProviderSettings(),
    onSuccess: () => invalidateAll(),
  });
}

/**
 * Tauri event name the backend emits after any write to the encrypted
 * agent-provider envelope (save / set-default / delete-profile / delete-all).
 * The event is broadcast to all windows so a second open Sprout window
 * (e.g. devtools detached or Settings open in another) sees the change
 * without a manual refresh.
 */
export const AGENT_PROVIDER_SETTINGS_CHANGED_EVENT =
  "agent-provider-settings:changed";

/**
 * Mount once near the top of the React tree (inside the QueryClientProvider).
 * Subscribes to the cross-window invalidation event and bumps every
 * agent-provider-related query. Cheap, fire-and-forget.
 */
export function useAgentProviderSettingsBroadcastSync() {
  const queryClient = useQueryClient();
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen(AGENT_PROVIDER_SETTINGS_CHANGED_EVENT, () => {
      void queryClient.invalidateQueries({
        queryKey: AGENT_PROVIDER_SETTINGS_STATE_KEY,
      });
      void queryClient.invalidateQueries({
        queryKey: AGENT_PROVIDER_PROFILE_KEY,
      });
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [queryClient]);
}
