import type * as React from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { initializeIdentity } from "@/shared/api/tauri";
import { relayClient } from "@/shared/api/relayClient";
import type { Identity } from "@/shared/api/types";

type IdentityGateProps = {
  children: React.ReactNode;
};

export function IdentityGate({ children }: IdentityGateProps) {
  const queryClient = useQueryClient();
  const identityInit = useQuery({
    queryKey: ["identity-init"],
    queryFn: async () => {
      const result = await initializeIdentity();

      relayClient.configure({ authMode: result.wsAuthMode });

      queryClient.setQueryData<Identity>(["identity"], {
        pubkey: result.pubkey,
        displayName: result.displayName,
      });

      return result;
    },
    staleTime: Number.POSITIVE_INFINITY,
    retry: 2,
  });

  if (identityInit.isPending) {
    return (
      <div className="flex h-dvh items-center justify-center">
        <p className="text-sm text-muted-foreground">Connecting…</p>
      </div>
    );
  }

  if (identityInit.isError) {
    const errorMsg =
      identityInit.error instanceof Error
        ? identityInit.error.message
        : String(identityInit.error);
    const isNetworkOrParseError =
      /failed to parse bootstrap response|error decoding|request failed|connection|timed? ?out|dns|resolve/i.test(
        errorMsg,
      );

    return (
      <div className="flex h-dvh flex-col items-center justify-center gap-4">
        <p className="text-sm text-destructive">
          Failed to initialize identity.
        </p>
        {isNetworkOrParseError ? (
          <p className="max-w-md text-center text-xs text-muted-foreground">
            Could not reach the relay. Make sure you are connected to Cloudflare
            WARP and try again.
          </p>
        ) : (
          <p className="max-w-md text-center text-xs text-muted-foreground">
            {errorMsg}
          </p>
        )}
        <button
          className="rounded-md bg-primary px-4 py-2 text-sm text-primary-foreground hover:bg-primary/90"
          onClick={() => void identityInit.refetch()}
          type="button"
        >
          Retry
        </button>
      </div>
    );
  }

  return children;
}
