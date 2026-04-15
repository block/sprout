import { useCallback, useEffect, useRef, useState } from "react";
import { QRCodeSVG } from "qrcode.react";
import { Check, Copy, Loader2, Smartphone, TriangleAlert } from "lucide-react";

import { useMintTokenMutation } from "@/features/tokens/hooks";
import { getNsec, getRelayHttpUrl } from "@/shared/api/tauri";
import type { TokenScope } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";

const MOBILE_SCOPES: TokenScope[] = [
  "messages:read",
  "messages:write",
  "channels:read",
  "users:read",
  "files:read",
];
const EXPIRES_IN_DAYS = 90;

function toBase64Url(str: string): string {
  return btoa(str).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

type PairingPayload = {
  relayUrl: string;
  token: string;
  pubkey: string;
  nsec: string;
};

function PairingDialog({
  open,
  onOpenChange,
  currentPubkey,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  currentPubkey: string;
}) {
  const mintMutation = useMintTokenMutation();
  const mintRef = useRef(mintMutation.mutateAsync);
  mintRef.current = mintMutation.mutateAsync;

  const [pairingUri, setPairingUri] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const generate = useCallback(async (pubkey: string) => {
    const [tokenResult, relayUrl, nsec] = await Promise.all([
      mintRef.current({
        name: `mobile-${Date.now()}`,
        scopes: [...MOBILE_SCOPES],
        expiresInDays: EXPIRES_IN_DAYS,
      }),
      getRelayHttpUrl(),
      getNsec(),
    ]);

    const payload: PairingPayload = {
      relayUrl,
      token: tokenResult.token,
      pubkey,
      nsec,
    };
    return `sprout://${toBase64Url(JSON.stringify(payload))}`;
  }, []);

  useEffect(() => {
    if (!open) return;

    setPairingUri(null);
    setError(null);
    setCopied(false);

    let cancelled = false;

    generate(currentPubkey).then(
      (uri) => {
        if (!cancelled) setPairingUri(uri);
      },
      (err) => {
        if (!cancelled) {
          setError(
            err instanceof Error
              ? err.message
              : "Failed to generate pairing code",
          );
        }
      },
    );

    return () => {
      cancelled = true;
    };
  }, [open, currentPubkey, generate]);

  async function handleCopy() {
    if (!pairingUri) return;
    await navigator.clipboard.writeText(pairingUri);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent
        className="max-w-md overflow-hidden p-0"
        data-testid="mobile-pairing-dialog"
      >
        <div className="flex max-h-[85vh] flex-col">
          <DialogHeader className="border-b border-border/60 px-6 py-5 pr-14">
            <DialogTitle>Pair Mobile Device</DialogTitle>
            <DialogDescription>
              Scan this QR code with the Sprout mobile app, or copy the pairing
              code for manual setup.
            </DialogDescription>
          </DialogHeader>

          <div className="flex-1 overflow-y-auto px-6 py-4">
            {error ? (
              <div className="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                <TriangleAlert className="mt-0.5 h-4 w-4 shrink-0" />
                <span>{error}</span>
              </div>
            ) : !pairingUri ? (
              <div className="flex flex-col items-center justify-center gap-3 py-8">
                <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
                <p className="text-sm text-muted-foreground">
                  Generating pairing code…
                </p>
              </div>
            ) : (
              <div className="space-y-4">
                <div className="flex justify-center rounded-lg border border-border/70 bg-white p-4">
                  <QRCodeSVG
                    data-testid="mobile-pairing-qr"
                    level="M"
                    size={240}
                    value={pairingUri}
                  />
                </div>

                <div className="space-y-1.5">
                  <p className="text-xs font-medium text-muted-foreground">
                    Pairing code
                  </p>
                  <div className="flex items-center gap-2">
                    <code className="min-w-0 flex-1 break-all rounded-lg border border-border bg-muted/50 px-3 py-2 text-xs">
                      {pairingUri}
                    </code>
                    <Button
                      data-testid="copy-pairing-code"
                      onClick={handleCopy}
                      size="sm"
                      variant="outline"
                    >
                      {copied ? (
                        <Check className="h-3.5 w-3.5" />
                      ) : (
                        <Copy className="h-3.5 w-3.5" />
                      )}
                    </Button>
                  </div>
                </div>

                <p className="text-xs text-muted-foreground">
                  This token expires in {EXPIRES_IN_DAYS} days. You can revoke
                  it from the Tokens settings at any time.
                </p>
              </div>
            )}
          </div>

          <div className="flex justify-end border-t border-border/60 bg-background/95 px-6 py-4">
            <Button
              data-testid="mobile-pairing-done"
              onClick={() => onOpenChange(false)}
              size="sm"
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

export function MobilePairingCard({
  currentPubkey,
}: {
  currentPubkey?: string;
}) {
  const [dialogOpen, setDialogOpen] = useState(false);

  return (
    <section className="min-w-0 space-y-3" data-testid="settings-mobile">
      <div className="space-y-1">
        <h2 className="text-sm font-semibold tracking-tight">Mobile</h2>
        <p className="text-sm text-muted-foreground">
          Connect the Sprout mobile app to this relay by scanning a QR code or
          pasting a pairing code.
        </p>
      </div>

      <div className="flex items-center gap-3 rounded-xl border border-border/80 bg-muted/25 px-4 py-3">
        <Smartphone className="h-5 w-5 text-muted-foreground" />
        <div className="flex-1">
          <p className="text-sm font-medium">Pair Mobile Device</p>
          <p className="text-xs text-muted-foreground">
            Generate a one-time pairing code for the mobile app
          </p>
        </div>
        <Button
          data-testid="pair-mobile-button"
          disabled={!currentPubkey}
          onClick={() => setDialogOpen(true)}
          size="sm"
        >
          Pair
        </Button>
      </div>

      {currentPubkey && (
        <PairingDialog
          currentPubkey={currentPubkey}
          onOpenChange={setDialogOpen}
          open={dialogOpen}
        />
      )}
    </section>
  );
}
