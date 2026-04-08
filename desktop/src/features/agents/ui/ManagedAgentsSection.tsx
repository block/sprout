import type { ManagedAgent, PresenceLookup } from "@/shared/api/types";
import { Skeleton } from "@/shared/ui/skeleton";
import { CreateNewButton } from "./CreateNewButton";
import { ManagedAgentRow } from "./ManagedAgentRow";

export function ManagedAgentsSection({
  actionErrorMessage,
  actionNoticeMessage,
  agents,
  error,
  isActionPending,
  isLoading,
  logContent,
  logError,
  logLoading,
  personaLabelsById,
  presenceLookup,
  onAddToChannel,
  onCreate,
  onDelete,
  onMintToken,
  onSelectLogAgent,
  onStart,
  onStop,
  onToggleStartOnAppLaunch,
  selectedLogAgentPubkey,
}: {
  actionErrorMessage: string | null;
  actionNoticeMessage: string | null;
  agents: ManagedAgent[];
  error: Error | null;
  isActionPending: boolean;
  isLoading: boolean;
  logContent: string | null;
  logError: Error | null;
  logLoading: boolean;
  personaLabelsById: Record<string, string>;
  presenceLookup: PresenceLookup;
  onAddToChannel: (agent: ManagedAgent) => void;
  onCreate: () => void;
  onDelete: (pubkey: string) => void;
  onMintToken: (pubkey: string, name: string) => void;
  onSelectLogAgent: (pubkey: string | null) => void;
  onStart: (pubkey: string) => void;
  onStop: (pubkey: string) => void;
  onToggleStartOnAppLaunch: (pubkey: string, startOnAppLaunch: boolean) => void;
  selectedLogAgentPubkey: string | null;
}) {
  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold tracking-tight">
            Managed agents
          </h3>
          <p className="text-sm text-muted-foreground">
            Agent profiles and process state — local and remote.
          </p>
        </div>
        <CreateNewButton
          ariaLabel="Create agent"
          label="Agent"
          onClick={onCreate}
        />
      </div>

      {isLoading ? (
        <div className="overflow-hidden rounded-xl border border-border/70 bg-card/80 shadow-sm">
          {["first", "second"].map((key) => (
            <div
              className="flex items-center gap-4 border-b border-border/60 px-4 py-3 last:border-b-0"
              key={key}
            >
              <Skeleton className="h-4 w-28" />
              <Skeleton className="h-5 w-16 rounded-full" />
              <Skeleton className="h-4 w-24" />
              <Skeleton className="h-4 w-20" />
            </div>
          ))}
        </div>
      ) : null}

      {!isLoading && agents.length === 0 ? (
        <div className="rounded-xl border border-dashed border-border/80 bg-card/70 px-6 py-10 text-center">
          <p className="text-sm font-semibold tracking-tight">
            No local agents yet
          </p>
          <p className="mt-2 text-sm text-muted-foreground">
            Create one to generate a keypair, mint a token, and launch the ACP
            harness from the desktop app.
          </p>
        </div>
      ) : null}

      {!isLoading && agents.length > 0 ? (
        <div className="space-y-2" data-testid="managed-agents-table">
          {agents.map((agent) => (
            <ManagedAgentRow
              agent={agent}
              isActionPending={isActionPending}
              isLogSelected={selectedLogAgentPubkey === agent.pubkey}
              key={agent.pubkey}
              logContent={
                selectedLogAgentPubkey === agent.pubkey ? logContent : null
              }
              logError={
                selectedLogAgentPubkey === agent.pubkey ? logError : null
              }
              logLoading={selectedLogAgentPubkey === agent.pubkey && logLoading}
              personaLabelsById={personaLabelsById}
              presenceLookup={presenceLookup}
              onAddToChannel={onAddToChannel}
              onDelete={onDelete}
              onMintToken={onMintToken}
              onSelectLogAgent={onSelectLogAgent}
              onStart={onStart}
              onStop={onStop}
              onToggleStartOnAppLaunch={onToggleStartOnAppLaunch}
            />
          ))}
        </div>
      ) : null}

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
