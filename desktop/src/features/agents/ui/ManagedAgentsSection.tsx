import { Plus, RefreshCcw } from "lucide-react";

import type { ManagedAgent } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import { Skeleton } from "@/shared/ui/skeleton";
import { ManagedAgentCard } from "./ManagedAgentCard";

export function ManagedAgentsSection({
  actionErrorMessage,
  actionNoticeMessage,
  agents,
  error,
  isActionPending,
  isLoading,
  selectedAgentPubkey,
  onAddToChannel,
  onCreate,
  onDelete,
  onMintToken,
  onRefresh,
  onSelect,
  onStart,
  onStop,
}: {
  actionErrorMessage: string | null;
  actionNoticeMessage: string | null;
  agents: ManagedAgent[];
  error: Error | null;
  isActionPending: boolean;
  isLoading: boolean;
  selectedAgentPubkey: string | null;
  onAddToChannel: (agent: ManagedAgent) => void;
  onCreate: () => void;
  onDelete: (pubkey: string) => void;
  onMintToken: (pubkey: string, name: string) => void;
  onRefresh: () => void;
  onSelect: (pubkey: string) => void;
  onStart: (pubkey: string) => void;
  onStop: (pubkey: string) => void;
}) {
  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold tracking-tight">
            Managed locally
          </h3>
          <p className="text-sm text-muted-foreground">
            Saved agent profiles and local ACP process state.
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button onClick={onCreate} type="button">
            <Plus className="h-4 w-4" />
            Create agent
          </Button>
          <Button onClick={onRefresh} type="button" variant="outline">
            <RefreshCcw className="h-4 w-4" />
            Refresh
          </Button>
        </div>
      </div>

      {isLoading ? (
        <div className="grid gap-3">
          {["first", "second"].map((key) => (
            <div
              className="rounded-3xl border border-border/70 bg-card/80 p-4"
              key={key}
            >
              <Skeleton className="h-5 w-32" />
              <Skeleton className="mt-3 h-4 w-48" />
              <Skeleton className="mt-4 h-16 w-full" />
            </div>
          ))}
        </div>
      ) : null}

      {!isLoading && agents.length === 0 ? (
        <div className="rounded-3xl border border-dashed border-border/80 bg-card/70 px-6 py-10 text-center">
          <p className="text-sm font-semibold tracking-tight">
            No local agents yet
          </p>
          <p className="mt-2 text-sm text-muted-foreground">
            Create one to generate a keypair, mint a token, and launch the ACP
            harness from the desktop app.
          </p>
        </div>
      ) : null}

      {agents.map((agent) => (
        <ManagedAgentCard
          agent={agent}
          isSelected={selectedAgentPubkey === agent.pubkey}
          key={agent.pubkey}
          onAddToChannel={(managedAgent) => {
            if (!isActionPending) {
              onAddToChannel(managedAgent);
            }
          }}
          onDelete={(pubkey) => {
            if (!isActionPending) {
              onDelete(pubkey);
            }
          }}
          onMintToken={(pubkey, name) => {
            if (!isActionPending) {
              onMintToken(pubkey, name);
            }
          }}
          onSelect={onSelect}
          onStart={(pubkey) => {
            if (!isActionPending) {
              onStart(pubkey);
            }
          }}
          onStop={(pubkey) => {
            if (!isActionPending) {
              onStop(pubkey);
            }
          }}
        />
      ))}

      {error ? (
        <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {error.message}
        </p>
      ) : null}

      {actionNoticeMessage ? (
        <p className="rounded-2xl border border-primary/20 bg-primary/10 px-4 py-3 text-sm text-primary">
          {actionNoticeMessage}
        </p>
      ) : null}

      {actionErrorMessage ? (
        <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {actionErrorMessage}
        </p>
      ) : null}
    </section>
  );
}
