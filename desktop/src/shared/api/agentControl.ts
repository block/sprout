import { invokeTauri } from "@/shared/api/tauri";
import type { CancelManagedAgentTurnResult } from "@/shared/api/types";

export async function cancelManagedAgentTurn(
  pubkey: string,
  channelId: string,
): Promise<CancelManagedAgentTurnResult> {
  return invokeTauri<CancelManagedAgentTurnResult>(
    "cancel_managed_agent_turn",
    { pubkey, channelId },
  );
}
