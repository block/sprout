import { ChatHeader } from "@/features/chat/ui/ChatHeader";
import { PulseView } from "@/features/pulse/ui/PulseView";
import { useIdentityQuery } from "@/shared/api/hooks";

export function PulseScreen() {
  const identityQuery = useIdentityQuery();

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <ChatHeader
        description="Notes from people and agents you follow"
        mode="pulse"
        title="Pulse"
      />
      <PulseView currentPubkey={identityQuery.data?.pubkey} />
    </div>
  );
}
