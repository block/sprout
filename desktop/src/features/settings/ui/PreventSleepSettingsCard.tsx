import { Shield } from "lucide-react";
import { usePreventSleepContext } from "@/features/agents/usePreventSleep";
import { cn } from "@/shared/lib/cn";

export function PreventSleepSettingsCard() {
  const {
    enabled,
    setEnabled,
    active,
    hasRunningAgents,
    expired,
    clearExpired,
  } = usePreventSleepContext();

  return (
    <section className="min-w-0" data-testid="settings-agents">
      <div className="mb-3 min-w-0">
        <h2 className="text-sm font-semibold tracking-tight">Agents</h2>
        <p className="text-sm text-muted-foreground">
          Settings that affect how local managed agents run on this machine.
        </p>
      </div>

      <button
        aria-pressed={enabled}
        className={cn(
          "flex w-full flex-col items-start gap-2 rounded-xl border px-4 py-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
          enabled
            ? "border-primary bg-primary/10 text-foreground"
            : "border-border/80 bg-background/60 text-muted-foreground hover:bg-accent hover:text-accent-foreground",
        )}
        data-testid="prevent-sleep-toggle"
        onClick={() => {
          if (expired) {
            clearExpired();
          }
          setEnabled(!enabled);
        }}
        type="button"
      >
        <div className="flex items-center gap-2">
          <Shield className="h-4 w-4" />
          <span className="font-medium text-foreground">
            Keep awake while agents are active
          </span>
        </div>
        <p className="text-sm text-muted-foreground">
          Prevents your computer from sleeping while local agents are running.
          Automatically releases when all agents stop or after 4 hours.
        </p>
      </button>

      <div className="mt-3 flex items-center gap-2 text-sm">
        <span
          className={cn(
            "inline-flex items-center rounded-full border px-2.5 py-0.5 text-xs font-semibold uppercase tracking-[0.18em]",
            active
              ? "border-primary/30 bg-primary/10 text-primary"
              : "border-border/80 bg-muted text-muted-foreground",
          )}
          data-testid="prevent-sleep-status"
        >
          {active ? "Active" : "Inactive"}
        </span>
        {enabled && !hasRunningAgents && (
          <span className="text-muted-foreground">
            Waiting for agents to start
          </span>
        )}
      </div>

      {expired && (
        <p className="mt-3 rounded-xl border border-yellow-500/30 bg-yellow-500/10 px-3 py-2 text-sm text-yellow-700 dark:text-yellow-400">
          Sleep prevention expired after 4 hours. Toggle off and on to
          re-enable.
        </p>
      )}
    </section>
  );
}
