/**
 * Best-effort cleanup of channel-scoped managed agents.
 *
 * Previously, this module would auto-delete managed agent records when a
 * channel was deleted or a member was removed, relying on relay kind:10100
 * events to determine "orphan" status. However, relay data can be
 * stale/incomplete, causing agents that ARE still in other channels to be
 * incorrectly deleted — wiping all their customized settings.
 *
 * The fix: skip auto-deletion entirely. Agent records are cheap to keep, and
 * users can manually remove orphaned agents from the agents page.
 */

/**
 * No-op. Previously deleted managed agents when a channel was deleted, but
 * stale relay data caused agents in other channels to be incorrectly removed.
 * Agent records are now intentionally preserved.
 */
export async function cleanupChannelAgents(_channelId: string): Promise<void> {
  // Intentionally no-op — see module docstring.
}

/**
 * No-op. Previously deleted a managed agent if it appeared orphaned after
 * being removed from a channel, but stale relay data made this unreliable.
 * Agent records are now intentionally preserved.
 */
export async function cleanupManagedAgentIfOrphaned(
  _pubkey: string,
  _channelId?: string,
): Promise<void> {
  // Intentionally no-op — see module docstring.
}
