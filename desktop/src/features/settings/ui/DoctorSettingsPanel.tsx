import {
  AlertTriangle,
  CheckCircle2,
  RefreshCw,
  Stethoscope,
} from "lucide-react";

import { useAcpProvidersQuery } from "@/features/agents/hooks";
import { describeResolvedCommand } from "@/features/agents/ui/agentUi";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";

function StatusIcon({ available }: { available: boolean }) {
  return available ? (
    <CheckCircle2 className="h-4 w-4 text-status-added" />
  ) : (
    <AlertTriangle className="h-4 w-4 text-warning" />
  );
}

function ProviderRow({
  command,
  defaultArgs,
  label,
  providerId,
  resolvedPath,
}: {
  command: string;
  defaultArgs: string[];
  label: string;
  providerId: string;
  resolvedPath: string;
}) {
  return (
    <div
      className="flex items-start gap-3 rounded-xl border border-border/70 bg-background/80 px-4 py-3"
      data-testid={`doctor-provider-${providerId}`}
    >
      <div className="mt-0.5 shrink-0">
        <StatusIcon available={true} />
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <p className="text-sm font-semibold tracking-tight">{label}</p>
          <code className="rounded bg-muted px-1.5 py-0.5 text-[11px]">
            {command}
          </code>
        </div>
        <p className="mt-1 text-sm text-muted-foreground">
          Available via {describeResolvedCommand(command, resolvedPath)}.
        </p>
        {defaultArgs.length > 0 ? (
          <p className="mt-2 text-xs text-muted-foreground">
            Default args:{" "}
            <code className="font-mono">{defaultArgs.join(", ")}</code>
          </p>
        ) : null}
        <p className="mt-1 break-all font-mono text-[11px] text-muted-foreground/80">
          {resolvedPath}
        </p>
      </div>
    </div>
  );
}

export function DoctorSettingsPanel() {
  const providersQuery = useAcpProvidersQuery();
  const providers = providersQuery.data ?? [];
  const isRefreshing = providersQuery.isFetching;

  return (
    <section className="space-y-5" data-testid="settings-doctor">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <Stethoscope className="h-4 w-4 text-primary" />
            <h2 className="text-sm font-semibold tracking-tight">Doctor</h2>
          </div>
          <p className="mt-1 text-sm text-muted-foreground">
            Verify the ACP runtime commands available to the desktop app.
          </p>
        </div>

        <Button
          className="shrink-0"
          disabled={isRefreshing}
          onClick={() => {
            void providersQuery.refetch();
          }}
          size="sm"
          type="button"
          variant="outline"
        >
          <RefreshCw
            className={cn("h-4 w-4", isRefreshing && "animate-spin")}
          />
          Re-run
        </Button>
      </div>

      <div className="mt-5 space-y-4">
        <div className="rounded-xl border border-border/70 bg-muted/20 p-4">
          <h3 className="text-sm font-semibold tracking-tight">ACP runtimes</h3>
          <p className="mt-1 text-sm text-muted-foreground">
            Installed runtimes that the desktop app can offer in Create agent.
          </p>

          <div className="mt-4 space-y-2">
            {providersQuery.isLoading ? (
              <p className="text-sm text-muted-foreground">
                Looking for installed ACP runtimes...
              </p>
            ) : providers.length > 0 ? (
              providers.map((provider) => (
                <ProviderRow
                  command={provider.command}
                  defaultArgs={provider.defaultArgs}
                  key={provider.id}
                  label={provider.label}
                  providerId={provider.id}
                  resolvedPath={provider.binaryPath}
                />
              ))
            ) : (
              <div className="rounded-xl border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-warning">
                No known ACP runtime was detected on your PATH yet. You can
                still use a custom command in Create agent.
              </div>
            )}
          </div>

          {providersQuery.error instanceof Error ? (
            <p className="mt-4 rounded-xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
              {providersQuery.error.message}
            </p>
          ) : null}
        </div>
      </div>
    </section>
  );
}
