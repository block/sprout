import * as React from "react";
import {
  AlertTriangle,
  CheckCircle2,
  Circle,
  Download,
  ExternalLink,
  RefreshCw,
} from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";

import {
  useAcpProvidersQuery,
  useInstallAcpRuntimeMutation,
} from "@/features/agents/hooks";
import { describeResolvedCommand } from "@/features/agents/ui/agentUi";
import type { AcpProviderCatalogEntry } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { SettingsOptionGroup } from "./SettingsOptionGroup";

function StatusIcon({
  availability,
}: {
  availability: AcpProviderCatalogEntry["availability"];
}) {
  switch (availability) {
    case "available":
      return <CheckCircle2 className="h-4 w-4 text-status-added" />;
    case "adapter_missing":
      return <AlertTriangle className="h-4 w-4 text-warning" />;
    case "cli_missing":
      return <AlertTriangle className="h-4 w-4 text-warning" />;
    case "not_installed":
      return <Circle className="h-4 w-4 text-muted-foreground/50" />;
  }
}

function InstallActions({
  isInstalling,
  onInstall,
  provider,
}: {
  isInstalling: boolean;
  onInstall: () => void;
  provider: AcpProviderCatalogEntry;
}) {
  return (
    <div className="mt-2 flex items-center gap-2">
      {provider.canAutoInstall ? (
        <Button
          disabled={isInstalling}
          onClick={onInstall}
          size="sm"
          type="button"
          variant="outline"
        >
          {isInstalling ? (
            <RefreshCw className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <Download className="h-3.5 w-3.5" />
          )}
          {isInstalling ? "Installing..." : "Install"}
        </Button>
      ) : null}
      <button
        className="inline-flex items-center gap-1 text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
        onClick={() => void openUrl(provider.installInstructionsUrl)}
        type="button"
      >
        <ExternalLink className="h-3 w-3" />
        View instructions
      </button>
    </div>
  );
}

function ProviderRow({
  installError,
  installSuccess,
  isInstalling,
  onInstall,
  provider,
}: {
  installError: string | null;
  installSuccess: boolean;
  isInstalling: boolean;
  onInstall: () => void;
  provider: AcpProviderCatalogEntry;
}) {
  return (
    <div
      className={cn(
        "flex min-h-16 items-start gap-3 px-4 py-3 text-sm",
        provider.availability === "available"
          ? "bg-background/60"
          : provider.availability === "adapter_missing" ||
              provider.availability === "cli_missing"
            ? "bg-amber-500/5"
            : "bg-muted/20",
      )}
      data-testid={`doctor-provider-${provider.id}`}
    >
      <div className="mt-0.5 shrink-0">
        <StatusIcon availability={provider.availability} />
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <p className="text-sm font-medium">{provider.label}</p>
          {provider.command ? (
            <code className="rounded bg-muted px-1.5 py-0.5 text-[11px]">
              {provider.command}
            </code>
          ) : null}
        </div>

        {provider.availability === "available" &&
        provider.command &&
        provider.binaryPath ? (
          <>
            <p className="mt-1 text-sm font-normal text-muted-foreground">
              Available via{" "}
              {describeResolvedCommand(provider.command, provider.binaryPath)}.
            </p>
            {provider.defaultArgs.length > 0 ? (
              <p className="mt-2 text-xs text-muted-foreground">
                Default args:{" "}
                <code className="font-mono">
                  {provider.defaultArgs.join(", ")}
                </code>
              </p>
            ) : null}
            {provider.underlyingCliPath &&
            provider.underlyingCliPath !== provider.binaryPath ? (
              <div className="mt-1 space-y-0.5">
                <p className="break-all font-mono text-[11px] text-muted-foreground/80">
                  <span className="text-muted-foreground">CLI:</span>{" "}
                  {provider.underlyingCliPath}
                </p>
                <p className="break-all font-mono text-[11px] text-muted-foreground/80">
                  <span className="text-muted-foreground">ACP adapter:</span>{" "}
                  {provider.binaryPath}
                </p>
              </div>
            ) : (
              <>
                <p className="mt-1 break-all font-mono text-[11px] text-muted-foreground/80">
                  {provider.binaryPath}
                </p>
                <p className="mt-1 text-[11px] text-muted-foreground/60">
                  ACP support built-in — no separate adapter needed.
                </p>
              </>
            )}
          </>
        ) : provider.availability === "adapter_missing" ? (
          <>
            <p className="mt-1 text-sm font-normal text-muted-foreground">
              CLI detected at{" "}
              <code className="rounded bg-muted px-1 py-0.5 text-[11px]">
                {provider.underlyingCliPath ?? "unknown path"}
              </code>{" "}
              but ACP adapter not found.
            </p>
            <p className="mt-1 text-sm font-normal text-muted-foreground">
              {provider.installHint}
            </p>
            <InstallActions
              isInstalling={isInstalling}
              onInstall={onInstall}
              provider={provider}
            />
          </>
        ) : provider.availability === "cli_missing" ? (
          <>
            <p className="mt-1 text-sm font-normal text-muted-foreground">
              ACP adapter found at{" "}
              <code className="rounded bg-muted px-1 py-0.5 text-[11px]">
                {provider.binaryPath ?? "unknown path"}
              </code>{" "}
              but the {provider.label} CLI is not installed.
            </p>
            <p className="mt-1 text-sm font-normal text-muted-foreground">
              {provider.installHint}
            </p>
            <InstallActions
              isInstalling={isInstalling}
              onInstall={onInstall}
              provider={provider}
            />
          </>
        ) : (
          <>
            <p className="mt-1 text-sm font-normal text-muted-foreground">
              Not installed
            </p>
            <p className="mt-1 text-sm font-normal text-muted-foreground">
              {provider.installHint}
            </p>
            <InstallActions
              isInstalling={isInstalling}
              onInstall={onInstall}
              provider={provider}
            />
          </>
        )}

        {installSuccess && provider.availability !== "available" ? (
          <p className="mt-2 rounded-lg border border-green-500/30 bg-green-500/10 px-3 py-1.5 text-sm text-green-700 dark:text-green-400">
            Installed successfully!
          </p>
        ) : null}
        {installError ? (
          <p className="mt-2 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-1.5 text-sm text-destructive">
            {installError}
          </p>
        ) : null}
      </div>
    </div>
  );
}

export function DoctorSettingsPanel() {
  const providersQuery = useAcpProvidersQuery();
  const providers = providersQuery.data ?? [];
  const isRefreshing = providersQuery.isFetching;
  const installMutation = useInstallAcpRuntimeMutation();
  const [installResults, setInstallResults] = React.useState<
    Record<string, { success: boolean; error: string | null }>
  >({});

  function handleInstall(providerId: string) {
    setInstallResults((prev) => ({
      ...prev,
      [providerId]: { success: false, error: null },
    }));
    installMutation.mutate(providerId, {
      onSuccess: (result) => {
        if (result.success) {
          setInstallResults((prev) => ({
            ...prev,
            [providerId]: { success: true, error: null },
          }));
        } else {
          const lastStep = result.steps[result.steps.length - 1];
          setInstallResults((prev) => ({
            ...prev,
            [providerId]: {
              success: false,
              error: lastStep
                ? `Step "${lastStep.step}" failed: ${lastStep.stderr || lastStep.stdout || "unknown error"}`
                : "Install failed with no output.",
            },
          }));
        }
      },
      onError: (error) => {
        setInstallResults((prev) => ({
          ...prev,
          [providerId]: {
            success: false,
            error: error instanceof Error ? error.message : "Install failed.",
          },
        }));
      },
    });
  }

  return (
    <section data-testid="settings-doctor">
      <div className="mb-12 flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <h2 className="text-2xl font-semibold tracking-tight">Doctor</h2>
          <p className="mt-1 text-base font-normal text-muted-foreground">
            Verify the ACP runtime commands available to the desktop app.
          </p>
        </div>

        <Button
          className="shrink-0"
          disabled={isRefreshing}
          onClick={() => {
            setInstallResults({});
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

      <div className="space-y-5">
        <SettingsOptionGroup>
          <div className="px-4 py-3 text-sm">
            <h3 className="text-sm font-medium">Agent CLIs and ACP runtimes</h3>
            <p className="mt-1 text-sm font-normal text-muted-foreground">
              Installation status of supported agent CLIs and their ACP
              runtimes.
            </p>
          </div>

          {providersQuery.isLoading ? (
            <div className="px-4 py-3 text-sm font-normal text-muted-foreground">
              Looking for ACP runtimes...
            </div>
          ) : providers.length > 0 ? (
            providers.map((provider) => (
              <ProviderRow
                installError={installResults[provider.id]?.error ?? null}
                installSuccess={installResults[provider.id]?.success ?? false}
                isInstalling={
                  installMutation.isPending &&
                  installMutation.variables === provider.id
                }
                key={provider.id}
                onInstall={() => handleInstall(provider.id)}
                provider={provider}
              />
            ))
          ) : (
            <div className="bg-amber-500/10 px-4 py-3 text-sm text-warning">
              No known ACP runtimes found.
            </div>
          )}

          {providersQuery.error instanceof Error ? (
            <p className="bg-destructive/10 px-4 py-3 text-sm text-destructive">
              {providersQuery.error.message}
            </p>
          ) : null}
        </SettingsOptionGroup>
      </div>
    </section>
  );
}
