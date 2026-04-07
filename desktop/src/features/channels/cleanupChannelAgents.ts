/**
 * Best-effort cleanup of channel-scoped managed agents.
 *
 * Each agent added via the "Add agents" dialog is a dedicated managed-agent
 * record. If that agent is no longer present in any channel, the managed-agent
 * record should be removed as well.
 */
import {
  deleteManagedAgent,
  getChannelMembers,
  listManagedAgents,
  listRelayAgents,
} from "@/shared/api/tauri";

async function cleanupManagedAgentsByPubkey(
  pubkeys: readonly string[],
  options?: { ignoreChannelId?: string },
): Promise<void> {
  const normalizedPubkeys = new Set(
    pubkeys
      .map((pubkey) => pubkey.trim().toLowerCase())
      .filter((pubkey) => pubkey.length > 0),
  );

  if (normalizedPubkeys.size === 0) {
    return;
  }

  const [managedAgents, relayAgents] = await Promise.all([
    listManagedAgents(),
    listRelayAgents(),
  ]);

  const agentsToDelete = managedAgents.filter((agent) =>
    normalizedPubkeys.has(agent.pubkey.toLowerCase()),
  );

  // Delete orphaned agents (best-effort — don't block channel deletion).
  await Promise.allSettled(
    agentsToDelete
      .filter((agent) => {
        const relayAgent = relayAgents.find(
          (candidate) =>
            candidate.pubkey.toLowerCase() === agent.pubkey.toLowerCase(),
        );
        if (!relayAgent) {
          // Not found in relay — safe to delete.
          return true;
        }

        const activeChannelIds = relayAgent.channelIds.filter(
          (channelId) => channelId !== options?.ignoreChannelId,
        );
        return activeChannelIds.length === 0;
      })
      .map((agent) => deleteManagedAgent(agent.pubkey)),
  );
}

export async function cleanupManagedAgentIfOrphaned(
  pubkey: string,
  channelId?: string,
): Promise<void> {
  await cleanupManagedAgentsByPubkey([pubkey], {
    ignoreChannelId: channelId,
  });
}

export async function cleanupChannelAgents(channelId: string): Promise<void> {
  const members = await getChannelMembers(channelId);
  await cleanupManagedAgentsByPubkey(
    members.map((member) => member.pubkey),
    {
      ignoreChannelId: channelId,
    },
  );
}
