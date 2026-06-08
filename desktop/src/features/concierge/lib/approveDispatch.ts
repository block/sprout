import { getChannelMembers, sendChannelMessage } from "@/shared/api/tauri";
import type { Channel } from "@/shared/api/types";
import type { DispatchIntent } from "@/features/concierge/types";

/** Case-insensitive channel lookup by name. Pure — exported for tests. */
export function resolveDispatchChannel(
  channels: Channel[],
  name: string,
): Channel | undefined {
  const normalized = name.trim().replace(/^#/, "").toLowerCase();
  return channels.find(
    (channel) => channel.name.trim().toLowerCase() === normalized,
  );
}

/**
 * Execute an approved dispatch: post `@agent <instruction>` to the target
 * channel. Resolves the agent's pubkey from the channel member list so the
 * mention actually notifies; falls back to a plain-text mention when the
 * agent isn't a member (the post still lands, visibly, for the human to fix).
 */
export async function postDispatch(
  intent: DispatchIntent,
  channels: Channel[],
): Promise<void> {
  const channel = resolveDispatchChannel(channels, intent.channel);
  if (!channel) {
    throw new Error(`Channel #${intent.channel} was not found on this relay.`);
  }
  const members = await getChannelMembers(channel.id);
  const agentName = intent.agent.trim().toLowerCase();
  const mention = members.find(
    (member) => member.displayName?.trim().toLowerCase() === agentName,
  );
  await sendChannelMessage(
    channel.id,
    `@${intent.agent} ${intent.instruction}`,
    null,
    undefined,
    mention ? [mention.pubkey] : undefined,
  );
}
