import * as React from "react";

import { getIdentity } from "@/shared/api/tauri";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";

import {
  DEFAULT_PUBLIC_RELAYS,
  DEFAULT_SERVERLESS_RELAY,
  normalizeRelayList,
  relayListIncludes,
  toggleRelayInList,
} from "../defaultRelays";
import { initFirstWorkspace, deriveWorkspaceName } from "../workspaceStorage";

const LOCAL_RELAY_URL = "ws://localhost:3000";

type WelcomeSetupProps = {
  defaultRelayUrl: string;
  onComplete: () => void;
};

export function WelcomeSetup({
  defaultRelayUrl,
  onComplete,
}: WelcomeSetupProps) {
  const isInternalBuild = defaultRelayUrl !== LOCAL_RELAY_URL;
  const [relayUrl, setRelayUrl] = React.useState(defaultRelayUrl);
  const [serverless, setServerless] = React.useState(false);
  const [isConnecting, setIsConnecting] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const handleServerlessChange = React.useCallback(
    (checked: boolean) => {
      setServerless(checked);
      setError(null);
      // Offer a sensible public relay default when flipping into serverless.
      if (checked && (relayUrl.trim() === "" || relayUrl === defaultRelayUrl)) {
        setRelayUrl(DEFAULT_SERVERLESS_RELAY);
      }
    },
    [relayUrl, defaultRelayUrl],
  );

  const handleConnect = React.useCallback(async () => {
    const trimmedUrl = relayUrl.trim();
    if (!trimmedUrl) {
      setError("Please enter a relay URL.");
      return;
    }

    setIsConnecting(true);
    setError(null);

    try {
      // We snapshot only the pubkey for display purposes (workspace switcher
      // labels, etc.). The private key lives on disk in `identity.key` and
      // is the single source of truth — never copied into localStorage.
      const identity = await getIdentity();
      initFirstWorkspace(
        serverless ? normalizeRelayList(trimmedUrl) : trimmedUrl,
        identity.pubkey,
        serverless ? "serverless" : "sprout",
      );

      // The reload triggered by onComplete() will re-run useWorkspaceInit,
      // which calls applyWorkspace with the saved config. No need to apply here.
      onComplete();
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to connect. Try again.",
      );
      setIsConnecting(false);
    }
  }, [relayUrl, serverless, onComplete]);

  const workspaceName = React.useMemo(
    () => deriveWorkspaceName(relayUrl.trim() || LOCAL_RELAY_URL),
    [relayUrl],
  );

  return (
    <div className="flex min-h-dvh items-center justify-center bg-[radial-gradient(circle_at_top,hsl(var(--primary)/0.14),transparent_48%),linear-gradient(180deg,hsl(var(--background)),hsl(var(--muted)/0.55))] px-4 py-8">
      <div className="w-full max-w-sm rounded-[28px] border border-border/70 bg-background/92 p-8 shadow-2xl backdrop-blur-sm">
        <p className="text-xs font-medium uppercase tracking-[0.2em] text-muted-foreground">
          Sprout
        </p>
        <h1 className="mt-3 text-2xl font-semibold tracking-tight text-foreground">
          Welcome
        </h1>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          {isInternalBuild
            ? "Connect to your workspace to get started."
            : "Running a local relay? Connect now. Or enter a custom relay URL."}
        </p>

        <div className="mt-6 space-y-4">
          <label className="flex items-start gap-2.5 rounded-md border border-border/70 bg-muted/40 p-3">
            <input
              checked={serverless}
              className="mt-0.5 h-4 w-4 accent-primary"
              onChange={(e) => handleServerlessChange(e.target.checked)}
              type="checkbox"
            />
            <span className="flex flex-col gap-0.5 text-left">
              <span className="text-sm font-medium text-foreground">
                Serverless mode
              </span>
              <span className="text-xs text-muted-foreground">
                Use a generic public Nostr relay — no Sprout server. Channels,
                DMs, and agents only.
              </span>
            </span>
          </label>
          {!isInternalBuild || serverless ? (
            <div className="space-y-1.5">
              <label
                className="text-xs font-medium text-muted-foreground"
                htmlFor="relay-url"
              >
                Relay URL
              </label>
              <Input
                id="relay-url"
                onChange={(e) => {
                  setRelayUrl(e.target.value);
                  setError(null);
                }}
                placeholder={
                  serverless ? DEFAULT_SERVERLESS_RELAY : "ws://localhost:3000"
                }
                type="url"
                value={relayUrl}
              />
              {serverless ? (
                <>
                  <p className="text-xs text-muted-foreground">
                    Connects to all listed relays for redundancy. Tap to
                    add/remove.
                  </p>
                  <div className="flex flex-wrap gap-1.5 pt-0.5">
                    {DEFAULT_PUBLIC_RELAYS.map((relay) => {
                      const selected = relayListIncludes(relayUrl, relay);
                      return (
                        <button
                          className={`rounded-full border px-2 py-0.5 text-xs transition-colors ${
                            selected
                              ? "border-primary bg-primary/15 text-foreground"
                              : "border-border bg-muted/40 text-muted-foreground hover:border-primary/60 hover:text-foreground"
                          }`}
                          key={relay}
                          onClick={() => {
                            setRelayUrl(toggleRelayInList(relayUrl, relay));
                            setError(null);
                          }}
                          type="button"
                        >
                          {relay.replace("wss://", "")}
                        </button>
                      );
                    })}
                  </div>
                </>
              ) : null}
            </div>
          ) : null}

          {error ? <p className="text-sm text-destructive">{error}</p> : null}

          <Button
            className="w-full"
            disabled={isConnecting || !relayUrl.trim()}
            onClick={handleConnect}
            size="default"
            type="button"
          >
            {isConnecting
              ? "Connecting..."
              : isInternalBuild
                ? `Connect to ${workspaceName}`
                : "Connect"}
          </Button>
        </div>
      </div>
    </div>
  );
}
