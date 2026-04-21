import {
  AlertTriangle,
  CheckCircle2,
  RefreshCw,
  Sparkles,
  Stethoscope,
  TerminalSquare,
} from "lucide-react";
import * as React from "react";

import {
  useAcpProvidersQuery,
  useManagedAgentPrereqsQuery,
} from "@/features/agents/hooks";
import { describeResolvedCommand } from "@/features/agents/ui/agentUi";
import type { CommandAvailability } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";

function StatusIcon({ available }: { available: boolean }) {
  return available ? (
    <CheckCircle2 className="h-4 w-4 text-status-added" />
  ) : (
    <AlertTriangle className="h-4 w-4 text-warning" />
  );
}

function CommandCheckRow({
  availability,
  id,
  isLoading,
  label,
  purpose,
}: {
  availability: CommandAvailability | null;
  id: string;
  isLoading: boolean;
  label: string;
  purpose: string;
}) {
  const command = availability?.command ?? "Unavailable";
  const isAvailable = availability?.available ?? false;

  return (
    <div
      className="flex items-start gap-3 rounded-xl border border-border/70 bg-background/80 px-4 py-3"
      data-testid={`doctor-check-${id}`}
    >
      <div className="mt-0.5 shrink-0">
        <StatusIcon available={isAvailable} />
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <p className="text-sm font-semibold tracking-tight">{label}</p>
          <code className="rounded bg-muted px-1.5 py-0.5 text-[11px]">
            {command}
          </code>
        </div>
        <p className="mt-1 text-sm text-muted-foreground">{purpose}</p>
        <p
          className={cn(
            "mt-2 text-xs",
            isAvailable ? "text-muted-foreground" : "text-warning",
          )}
        >
          {availability?.resolvedPath
            ? `Available via ${describeResolvedCommand(command, availability.resolvedPath)}`
            : isLoading
              ? "Checking for a matching binary..."
              : "Not currently available."}
        </p>
        {availability?.resolvedPath ? (
          <p className="mt-1 break-all font-mono text-[11px] text-muted-foreground/80">
            {availability.resolvedPath}
          </p>
        ) : null}
      </div>
    </div>
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

function SetupHelpCard() {
  return (
    <div className="rounded-xl border border-border/70 bg-muted/20 p-4">
      <div className="flex items-center gap-2">
        <Sparkles className="h-4 w-4 text-primary" />
        <h3 className="text-sm font-semibold tracking-tight">Setup help</h3>
      </div>

      <div className="mt-3 space-y-3 text-sm text-muted-foreground">
        <p>
          Build the local Sprout tools with{" "}
          <code className="rounded bg-background px-1.5 py-0.5 font-mono text-[12px]">
            cargo build --release --workspace
          </code>{" "}
          when you want the desktop app to spawn ACP harnesses from this
          checkout.
        </p>
        <p>
          If you keep binaries outside your PATH, use the custom ACP and MCP
          commands below and then copy those same values into Create agent &gt;
          Advanced setup.
        </p>
        <p>
          ACP runtimes like Goose or Codex are optional. They appear
          automatically once their commands are installed on your PATH.
        </p>
      </div>
    </div>
  );
}

export function DoctorSettingsPanel() {
  const [acpCommand, setAcpCommand] = React.useState("sprout-acp");
  const [mcpCommand, setMcpCommand] = React.useState("sprout-mcp-server");
  const providersQuery = useAcpProvidersQuery();
  const prereqsQuery = useManagedAgentPrereqsQuery(acpCommand, mcpCommand);
  const prereqs = prereqsQuery.data ?? null;
  const providers = providersQuery.data ?? [];
  const isRefreshing = providersQuery.isFetching || prereqsQuery.isFetching;

  const toolChecks = [
    {
      id: "acp",
      label: "ACP harness",
      purpose:
        "Desktop launches this command to bridge a local runtime into ACP.",
      availability: prereqs?.acp ?? null,
    },
    {
      id: "mcp",
      label: "MCP server",
      purpose:
        "Desktop uses this server when the ACP harness requests Sprout tools.",
      availability: prereqs?.mcp ?? null,
    },
  ];

  const hasMissingSproutTools =
    prereqs !== null && (!prereqs.acp.available || !prereqs.mcp.available);

  return (
    <section className="space-y-5" data-testid="settings-doctor">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <Stethoscope className="h-4 w-4 text-primary" />
            <h2 className="text-sm font-semibold tracking-tight">Doctor</h2>
          </div>
          <p className="mt-1 text-sm text-muted-foreground">
            Verify the local Sprout tools and ACP runtime commands used by the
            desktop app.
          </p>
        </div>

        <Button
          className="shrink-0"
          disabled={isRefreshing}
          onClick={() => {
            void providersQuery.refetch();
            void prereqsQuery.refetch();
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

      <div className="mt-5 grid gap-4">
        <div className="space-y-4">
          <div className="rounded-xl border border-border/70 bg-muted/20 p-4">
            <div className="flex items-center gap-2">
              <TerminalSquare className="h-4 w-4 text-primary" />
              <h3 className="text-sm font-semibold tracking-tight">
                Local Sprout binaries
              </h3>
            </div>
            <p className="mt-1 text-sm text-muted-foreground">
              These checks replace the old binary status card from Create agent.
            </p>

            <div className="mt-4 space-y-2">
              {toolChecks.map((check) => (
                <CommandCheckRow
                  availability={check.availability}
                  id={check.id}
                  isLoading={prereqsQuery.isLoading}
                  key={check.id}
                  label={check.label}
                  purpose={check.purpose}
                />
              ))}
            </div>

            {hasMissingSproutTools ? (
              <p className="mt-4 rounded-xl border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-warning">
                Build the workspace binaries with{" "}
                <code className="font-mono">
                  cargo build --release --workspace
                </code>{" "}
                or point agent creation at custom ACP and MCP commands.
              </p>
            ) : null}

            {prereqsQuery.error instanceof Error ? (
              <p className="mt-4 rounded-xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {prereqsQuery.error.message}
              </p>
            ) : null}
          </div>

          <div className="rounded-xl border border-border/70 bg-muted/20 p-4">
            <h3 className="text-sm font-semibold tracking-tight">
              Custom harness commands
            </h3>
            <p className="mt-1 text-sm text-muted-foreground">
              Verify non-default ACP or MCP binaries before using them in agent
              creation.
            </p>

            <div className="mt-4 grid gap-4 md:grid-cols-2">
              <div className="space-y-1.5">
                <label className="text-sm font-medium" htmlFor="doctor-acp">
                  ACP command
                </label>
                <Input
                  data-testid="doctor-acp-command"
                  id="doctor-acp"
                  onChange={(event) => setAcpCommand(event.target.value)}
                  value={acpCommand}
                />
              </div>

              <div className="space-y-1.5">
                <label className="text-sm font-medium" htmlFor="doctor-mcp">
                  MCP command
                </label>
                <Input
                  data-testid="doctor-mcp-command"
                  id="doctor-mcp"
                  onChange={(event) => setMcpCommand(event.target.value)}
                  value={mcpCommand}
                />
              </div>
            </div>
          </div>
        </div>

        <div className="space-y-4">
          <div className="rounded-xl border border-border/70 bg-muted/20 p-4">
            <h3 className="text-sm font-semibold tracking-tight">
              ACP runtimes
            </h3>
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

          <SetupHelpCard />
        </div>
      </div>
    </section>
  );
}
