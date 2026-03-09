import { Paperclip, SendHorizontal, SmilePlus } from "lucide-react";

import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";

type MessageComposerProps = {
  channelName: string;
};

export function MessageComposer({ channelName }: MessageComposerProps) {
  return (
    <footer className="border-t border-border/80 bg-background p-4">
      <div className="mx-auto flex w-full max-w-4xl flex-col gap-3">
        <div className="rounded-2xl border border-input bg-card px-3 py-4 shadow-sm sm:px-4">
          <Input
            aria-label="Message channel"
            className="h-auto border-0 bg-transparent px-0 py-0 text-sm leading-6 shadow-none focus-visible:ring-0"
            placeholder={`Message #${channelName}`}
          />

          <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              <Button size="icon" type="button" variant="ghost">
                <Paperclip className="h-4 w-4" />
              </Button>
              <Button size="icon" type="button" variant="ghost">
                <SmilePlus className="h-4 w-4" />
              </Button>
            </div>

            <Button className="gap-2" type="button">
              <SendHorizontal className="h-4 w-4" />
              Send
            </Button>
          </div>
        </div>
      </div>
    </footer>
  );
}
