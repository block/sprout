import * as React from "react";
import { Check, Copy, KeyRound, ShieldX } from "lucide-react";

import { pubkeyToNpub } from "@/shared/lib/nostrUtils";
import { Badge } from "@/shared/ui/badge";
import { Button } from "@/shared/ui/button";

type MembershipDeniedProps = {
  onChangeKey?: () => void;
  onRetry: () => void;
  pubkey: string;
};

export function MembershipDenied({
  onChangeKey,
  onRetry,
  pubkey,
}: MembershipDeniedProps) {
  const npub = React.useMemo(() => {
    if (!pubkey) {
      return "Unknown public key";
    }

    try {
      return pubkeyToNpub(pubkey);
    } catch {
      return pubkey;
    }
  }, [pubkey]);
  const [copied, setCopied] = React.useState(false);

  const handleCopy = React.useCallback(async () => {
    try {
      await navigator.clipboard.writeText(npub);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Fallback: select the text so the user can copy manually
    }
  }, [npub]);

  return (
    <div className="flex min-h-dvh items-center justify-center bg-[radial-gradient(circle_at_top,hsl(var(--primary)/0.14),transparent_48%),linear-gradient(180deg,hsl(var(--background)),hsl(var(--muted)/0.55))] px-4 py-8">
      <div className="w-full max-w-md rounded-[28px] border border-border/70 bg-background/92 p-8 shadow-2xl backdrop-blur">
        <div className="space-y-3">
          <Badge variant="warning">Membership required</Badge>
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-destructive/10">
              <ShieldX className="h-5 w-5 text-destructive" />
            </div>
            <h1 className="text-2xl font-semibold tracking-tight text-foreground">
              Not a member yet
            </h1>
          </div>
          <p className="text-sm leading-6 text-muted-foreground">
            This relay requires an invitation. Ask a relay admin to add you as a
            member, then come back and try again.
          </p>
        </div>

        <div className="mt-6 space-y-3">
          <div className="space-y-1.5">
            <p className="text-xs font-medium text-muted-foreground">
              Your public key (npub)
            </p>
            <div className="flex items-center gap-2">
              <code className="min-w-0 flex-1 truncate rounded-xl border border-border/70 bg-muted/30 px-3 py-2.5 font-mono text-xs text-foreground">
                {npub}
              </code>
              <Button
                className="shrink-0"
                onClick={() => {
                  void handleCopy();
                }}
                size="icon"
                title="Copy npub"
                type="button"
                variant="outline"
              >
                {copied ? (
                  <Check className="h-4 w-4 text-emerald-500" />
                ) : (
                  <Copy className="h-4 w-4" />
                )}
              </Button>
            </div>
          </div>
          <p className="text-xs leading-5 text-muted-foreground">
            This is your public identity — it&apos;s safe to share. Send it to
            the relay admin so they can invite you.
          </p>
        </div>

        <div className="mt-6 flex flex-col gap-2">
          <Button className="w-full" onClick={onRetry} type="button">
            Try again
          </Button>
          {onChangeKey ? (
            <button
              className="flex w-full items-center justify-center gap-1.5 text-xs text-muted-foreground transition-colors hover:text-foreground"
              onClick={onChangeKey}
              type="button"
            >
              <KeyRound className="h-3 w-3" />
              Use a different key
            </button>
          ) : null}
        </div>
      </div>
    </div>
  );
}
