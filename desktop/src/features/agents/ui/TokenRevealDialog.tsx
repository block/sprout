import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { CopyButton } from "./CopyButton";

export function TokenRevealDialog({
  name,
  token,
  onOpenChange,
}: {
  name: string | null;
  token: string | null;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog onOpenChange={onOpenChange} open={token !== null}>
      <DialogContent className="max-w-xl overflow-hidden p-0">
        <div className="flex max-h-[85vh] flex-col">
          <DialogHeader className="border-b border-border/60 px-6 py-5 pr-14">
            <DialogTitle>Agent token minted</DialogTitle>
            <DialogDescription>
              Save this token now. Restart the harness if you want the running
              agent to pick it up immediately.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 px-6 py-5">
            <div className="rounded-2xl border border-border/70 bg-muted/20 p-4">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <p className="text-sm font-semibold tracking-tight">{name}</p>
                  <p className="text-sm text-muted-foreground">
                    Token shown once only.
                  </p>
                </div>
                {token ? <CopyButton label="Copy token" value={token} /> : null}
              </div>
              {token ? (
                <code className="mt-3 block break-all rounded-xl border border-border/70 bg-background/80 px-3 py-2 text-xs">
                  {token}
                </code>
              ) : null}
            </div>
          </div>

          <div className="flex justify-end border-t border-border/60 px-6 py-4">
            <Button
              onClick={() => onOpenChange(false)}
              size="sm"
              type="button"
              variant="outline"
            >
              Done
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
