import { invoke } from '@tauri-apps/api/core';
import { Bot } from 'lucide-react';
import * as React from 'react';

import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/shared/ui/dialog';

type ManagedAgentSummary = {
  pubkey: string;
  name: string;
  status: string;
};

type AgentAddResult = {
  ephemeral_added: boolean;
  parent_added: boolean;
  parent_error: string | null;
};

type AddAgentDialogProps = {
  onClose: () => void;
  onAdd: (pubkey: string) => Promise<AgentAddResult>;
};

export function AddAgentDialog({ onClose, onAdd }: AddAgentDialogProps) {
  const [agents, setAgents] = React.useState<ManagedAgentSummary[]>([]);
  const [loading, setLoading] = React.useState(true);
  const [adding, setAdding] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [warning, setWarning] = React.useState<string | null>(null);

  React.useEffect(() => {
    invoke<ManagedAgentSummary[]>('list_managed_agents')
      .then(setAgents)
      .catch((e: unknown) => {
        console.error('Failed to load agents:', e);
        setError('Could not load agents.');
      })
      .finally(() => setLoading(false));
  }, []);

  // Only show agents that are currently running.
  const runningAgents = agents.filter((a) => a.status === 'running');

  async function handleAdd(pubkey: string) {
    if (adding) return;
    setAdding(pubkey);
    setError(null);
    setWarning(null);
    try {
      const result = await onAdd(pubkey);
      if (result.parent_error) {
        // Agent was added to the ephemeral channel but parent channel add failed.
        // Show as a warning — don't close the dialog so the user can see it.
        setWarning(`Added to huddle, but parent channel failed: ${result.parent_error}`);
      } else {
        onClose();
      }
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(`Failed to add agent: ${msg}`);
      console.error('Failed to add agent to huddle:', e);
    } finally {
      setAdding(null);
    }
  }

  return (
    <Dialog onOpenChange={(open) => { if (!open) onClose(); }} open>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>Add Agent to Huddle</DialogTitle>
        </DialogHeader>

        {error && (
          <p className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {error}
          </p>
        )}

        {warning && (
          <div className="flex items-start justify-between gap-2 rounded-md bg-amber-500/10 px-3 py-2 text-sm text-amber-700 dark:text-amber-400">
            <span>{warning}</span>
            <button
              className="shrink-0 font-medium underline-offset-2 hover:underline"
              onClick={onClose}
              type="button"
            >
              Dismiss
            </button>
          </div>
        )}

        {loading ? (
          <p className="py-4 text-center text-sm text-muted-foreground">
            Loading agents…
          </p>
        ) : runningAgents.length === 0 ? (
          <p className="py-4 text-center text-sm text-muted-foreground">
            No running agents found.
          </p>
        ) : (
          <ul className="flex flex-col gap-1">
            {runningAgents.map((agent) => (
              <li key={agent.pubkey}>
                <button
                  className="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left text-sm transition-colors hover:bg-accent hover:text-accent-foreground disabled:opacity-50"
                  disabled={adding === agent.pubkey}
                  onClick={() => void handleAdd(agent.pubkey)}
                  type="button"
                >
                  <Bot className="h-4 w-4 shrink-0 text-muted-foreground" />
                  <span className="flex-1 truncate font-medium">{agent.name}</span>
                  <span className="shrink-0 text-xs text-muted-foreground">
                    {agent.status}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </DialogContent>
    </Dialog>
  );
}

export type { AgentAddResult };
