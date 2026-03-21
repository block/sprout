/**
 * Best-effort cleanup of managed agents when a channel is deleted.
 *
 * Each agent added via the "Add agents" dialog is a unique process scoped to
 * the channel. When the channel is deleted these orphaned agents should be
 * removed — but only if they are not members of any other channel.
 */
import {
  deleteManagedAgent,
  getChannelMembers,
  listManagedAgents,
  listRelayAgents,
} from "@/shared/api/tauri";

export async function cleanupChannelAgents(channelId: string): Promise<void> {
  const [members, managedAgents, relayAgents] = await Promise.all([
    getChannelMembers(channelId),
    listManagedAgents(),
    listRelayAgents(),
  ]);

  const memberPubkeys = new Set(
    members.map((member) => member.pubkey.toLowerCase()),
  );

  // Find managed agents that are members of this channel.
  const agentsInChannel = managedAgents.filter((agent) =>
    memberPubkeys.has(agent.pubkey.toLowerCase()),
  );

  // Only delete agents that are NOT members of any other channel.
  const agentsToDelete = agentsInChannel.filter((agent) => {
    const relayAgent = relayAgents.find(
      (ra) => ra.pubkey.toLowerCase() === agent.pubkey.toLowerCase(),
    );
    if (!relayAgent) {
      // Not found in relay — safe to delete.
      return true;
    }
    // Only delete if this is the agent's only channel.
    const otherChannels = relayAgent.channelIds.filter(
      (id) => id !== channelId,
    );
    return otherChannels.length === 0;
  });

  // Delete orphaned agents (best-effort — don't block channel deletion).
  await Promise.allSettled(
    agentsToDelete.map((agent) => deleteManagedAgent(agent.pubkey)),
  );
}
