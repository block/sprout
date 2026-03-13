import type { RelayAgent } from "@/shared/api/types";
import { Skeleton } from "@/shared/ui/skeleton";
import { RelayAgentCard } from "./RelayAgentCard";

export function RelayDirectorySection({
  error,
  isLoading,
  managedPubkeys,
  relayAgents,
}: {
  error: Error | null;
  isLoading: boolean;
  managedPubkeys: Set<string>;
  relayAgents: RelayAgent[];
}) {
  return (
    <section className="space-y-4">
      <div>
        <h3 className="text-sm font-semibold tracking-tight">
          Relay directory
        </h3>
        <p className="text-sm text-muted-foreground">
          Bot and agent identities visible to the current desktop user.
        </p>
      </div>

      {isLoading ? (
        <div className="grid gap-3">
          {["directory-1", "directory-2"].map((key) => (
            <div
              className="rounded-3xl border border-border/70 bg-card/80 p-4"
              key={key}
            >
              <Skeleton className="h-5 w-36" />
              <Skeleton className="mt-3 h-4 w-44" />
              <Skeleton className="mt-4 h-12 w-full" />
            </div>
          ))}
        </div>
      ) : null}

      {!isLoading && relayAgents.length === 0 ? (
        <div className="rounded-3xl border border-dashed border-border/80 bg-card/70 px-6 py-10 text-center">
          <p className="text-sm font-semibold tracking-tight">
            No relay-visible agents yet
          </p>
          <p className="mt-2 text-sm text-muted-foreground">
            Start one of your local harnesses or join an existing bot to a
            channel and it will appear here.
          </p>
        </div>
      ) : null}

      {relayAgents.map((agent) => (
        <RelayAgentCard
          agent={agent}
          isManagedLocally={managedPubkeys.has(agent.pubkey)}
          key={agent.pubkey}
        />
      ))}

      {error ? (
        <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {error.message}
        </p>
      ) : null}
    </section>
  );
}
