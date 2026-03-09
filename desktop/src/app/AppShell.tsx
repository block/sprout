import { currentChannel, messages } from "@/features/chat/data/chatData";
import { ChatHeader } from "@/features/chat/ui/ChatHeader";
import { MessageComposer } from "@/features/chat/ui/MessageComposer";
import { MessageTimeline } from "@/features/chat/ui/MessageTimeline";
import { AppSidebar } from "@/features/sidebar/ui/AppSidebar";
import { SidebarInset, SidebarProvider } from "@/shared/ui/sidebar";

export function AppShell() {
  return (
    <SidebarProvider className="h-dvh overflow-hidden overscroll-none">
      <AppSidebar />

      <SidebarInset className="min-h-0 min-w-0 overflow-hidden">
        <ChatHeader
          description={currentChannel.description}
          title={currentChannel.name}
        />

        <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          <MessageTimeline messages={messages} />
          <MessageComposer channelName={currentChannel.name} />
        </div>
      </SidebarInset>
    </SidebarProvider>
  );
}
