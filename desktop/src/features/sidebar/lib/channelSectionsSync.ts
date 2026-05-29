import { relayClient } from "@/shared/api/relayClient";
import {
  nip44DecryptFromSelf,
  nip44EncryptToSelf,
  signRelayEvent,
} from "@/shared/api/tauri";
import type { RelayEvent } from "@/shared/api/types";
import { KIND_CHANNEL_SECTIONS } from "@/shared/constants/kinds";
import type { ChannelSectionStore } from "./channelSectionsStorage";

const D_TAG = "channel-sections";
const DEBOUNCE_MS = 2_000;

export type RemoteSections = {
  store: ChannelSectionStore;
  createdAt: number;
};

let debounceTimer: number | null = null;
let lastRemoteCreatedAt = 0;

function parsePayload(json: unknown): ChannelSectionStore | null {
  if (typeof json !== "object" || json === null) return null;
  const obj = json as Record<string, unknown>;
  const sections = Array.isArray(obj.sections)
    ? obj.sections.filter(
        (e: unknown): e is { id: string; name: string; order: number } =>
          typeof e === "object" &&
          e !== null &&
          typeof (e as Record<string, unknown>).id === "string" &&
          typeof (e as Record<string, unknown>).name === "string" &&
          typeof (e as Record<string, unknown>).order === "number",
      )
    : [];
  const assignments =
    typeof obj.assignments === "object" &&
    obj.assignments !== null &&
    !Array.isArray(obj.assignments)
      ? Object.fromEntries(
          Object.entries(obj.assignments as Record<string, unknown>).filter(
            (e): e is [string, string] => typeof e[1] === "string",
          ),
        )
      : {};
  return { version: 1, sections, assignments };
}

async function decryptAndParse(
  event: RelayEvent,
): Promise<RemoteSections | null> {
  try {
    const plaintext = await nip44DecryptFromSelf(event.content);
    const store = parsePayload(JSON.parse(plaintext));
    if (!store) return null;
    return { store, createdAt: event.created_at };
  } catch {
    return null;
  }
}

export async function fetchRemoteSections(
  pubkey: string,
): Promise<RemoteSections | null> {
  try {
    const events = await relayClient.fetchEvents({
      kinds: [KIND_CHANNEL_SECTIONS],
      authors: [pubkey],
      "#d": [D_TAG],
      limit: 1,
    });
    if (events.length === 0) return null;
    const result = await decryptAndParse(events[0]);
    if (result) {
      lastRemoteCreatedAt = Math.max(lastRemoteCreatedAt, result.createdAt);
    }
    return result;
  } catch {
    return null;
  }
}

export function publishSections(store: ChannelSectionStore): void {
  if (debounceTimer !== null) {
    window.clearTimeout(debounceTimer);
  }
  debounceTimer = window.setTimeout(() => {
    debounceTimer = null;
    void doPublish(store);
  }, DEBOUNCE_MS);
}

async function doPublish(store: ChannelSectionStore): Promise<void> {
  try {
    const payload = {
      sections: store.sections,
      assignments: store.assignments,
    };
    const ciphertext = await nip44EncryptToSelf(JSON.stringify(payload));
    const createdAt = Math.max(
      Math.floor(Date.now() / 1_000),
      lastRemoteCreatedAt + 1,
    );
    const event = await signRelayEvent({
      kind: KIND_CHANNEL_SECTIONS,
      content: ciphertext,
      createdAt,
      tags: [
        ["d", D_TAG],
        ["t", D_TAG],
      ],
    });
    await relayClient.publishEvent(
      event,
      "Timed out publishing channel sections.",
      "Failed to publish channel sections.",
    );
    lastRemoteCreatedAt = Math.max(lastRemoteCreatedAt, event.created_at);
  } catch {
    // Non-fatal: next mutation or reconnect will retry
  }
}

export async function subscribeToSections(
  pubkey: string,
  onUpdate: (remote: RemoteSections) => void,
): Promise<() => Promise<void>> {
  return relayClient.subscribeLive(
    {
      kinds: [KIND_CHANNEL_SECTIONS],
      authors: [pubkey],
      "#d": [D_TAG],
      limit: 0,
    },
    (event: RelayEvent) => {
      void decryptAndParse(event).then((result) => {
        if (result) {
          lastRemoteCreatedAt = Math.max(lastRemoteCreatedAt, result.createdAt);
          onUpdate(result);
        }
      });
    },
  );
}

export function resetSyncState(): void {
  if (debounceTimer !== null) {
    window.clearTimeout(debounceTimer);
    debounceTimer = null;
  }
  lastRemoteCreatedAt = 0;
}
