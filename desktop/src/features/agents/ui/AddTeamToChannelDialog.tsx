import { AlertTriangle } from "lucide-react";
import * as React from "react";

import {
  useAcpProvidersQuery,
  useCreateChannelManagedAgentsMutation,
} from "@/features/agents/hooks";
import type { CreateChannelManagedAgentsResult } from "@/features/agents/channelAgents";
import { resolvePersonaProvider } from "@/features/agents/lib/resolvePersonaProvider";
import { useChannelsQuery } from "@/features/channels/hooks";
import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import type {
  AgentPersona,
  AgentTeam,
  Channel,
  ChannelRole,
} from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";

type AddTeamToChannelDialogProps = {
  team: AgentTeam | null;
  personas: AgentPersona[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onDeployed: (
    channel: Channel,
    result: CreateChannelManagedAgentsResult,
  ) => void;
};

function resolvePersonas(
  personaIds: string[],
  personas: AgentPersona[],
): AgentPersona[] {
  return personaIds
    .map((id) => personas.find((p) => p.id === id))
    .filter((p): p is AgentPersona => p !== undefined);
}

export function AddTeamToChannelDialog({
  team,
  personas,
  open,
  onOpenChange,
  onDeployed,
}: AddTeamToChannelDialogProps) {
  const channelsQuery = useChannelsQuery();
  const providersQuery = useAcpProvidersQuery();
  const [channelId, setChannelId] = React.useState("");
  const [role, setRole] = React.useState<Exclude<ChannelRole, "owner">>("bot");
  const deployMutation = useCreateChannelManagedAgentsMutation(
    channelId || null,
  );

  const channels = React.useMemo(
    () =>
      (channelsQuery.data ?? []).filter(
        (channel) => channel.channelType !== "dm" && !channel.archivedAt,
      ),
    [channelsQuery.data],
  );

  const providers = providersQuery.data ?? [];
  const defaultProvider = providers[0] ?? null;

  const resolved = team ? resolvePersonas(team.personaIds, personas) : [];

  // Surface warnings when a persona's preferred provider is unavailable.
  // This dialog has no provider selector, so the fallback is always
  // `defaultProvider` (the first available runtime).
  const providerWarnings = React.useMemo(() => {
    if (!defaultProvider) return [];
    const warnings: string[] = [];
    for (const persona of resolved) {
      const { warnings: w } = resolvePersonaProvider(
        persona.provider,
        providers,
        defaultProvider,
      );
      warnings.push(...w);
    }
    return warnings;
  }, [resolved, providers, defaultProvider]);

  function reset() {
    setChannelId("");
    setRole("bot");
    deployMutation.reset();
  }

  function handleOpenChange(next: boolean) {
    if (!next) {
      reset();
    }
    onOpenChange(next);
  }

  React.useEffect(() => {
    if (!open) {
      return;
    }
    if (!channelId && channels.length > 0) {
      setChannelId(channels[0].id);
    }
  }, [channelId, channels, open]);

  const selectedChannel =
    channels.find((channel) => channel.id === channelId) ?? null;

  async function handleDeploy() {
    if (!team || !selectedChannel || !defaultProvider) {
      return;
    }

    try {
      // Resolve each persona's preferred provider. This dialog has no
      // provider selector, so the fallback is `defaultProvider` (first
      // available runtime). Warnings are computed separately via the
      // `providerWarnings` memo and rendered as inline alerts above.
      const inputs = resolved.map((persona) => {
        const { provider: personaProvider } = resolvePersonaProvider(
          persona.provider,
          providers,
          defaultProvider,
        );
        const providerToUse = personaProvider ?? defaultProvider;
        return {
          provider: {
            id: providerToUse.id,
            label: providerToUse.label,
            command: providerToUse.command,
            defaultArgs: providerToUse.defaultArgs,
          },
          name: persona.displayName,
          systemPrompt: persona.systemPrompt,
          avatarUrl: persona.avatarUrl ?? undefined,
          model: persona.model ?? undefined,
          personaId: persona.id,
          role,
        };
      });

      const result = await deployMutation.mutateAsync(inputs);
      onDeployed(selectedChannel, result);
      handleOpenChange(false);
    } catch {
      // React Query stores the error; keep the dialog open.
    }
  }

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent className="max-w-xl overflow-hidden p-0">
        <div className="flex max-h-[85vh] flex-col">
          <DialogHeader className="border-b border-border/60 px-6 py-5 pr-14">
            <DialogTitle>Deploy team to channel</DialogTitle>
            <DialogDescription>
              Create and attach one agent per persona in{" "}
              <strong>{team?.name ?? "this team"}</strong> to the selected
              channel.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-5 px-6 py-5">
            {resolved.length > 0 ? (
              <div className="space-y-1.5">
                <span className="text-sm font-medium">
                  Personas ({resolved.length})
                </span>
                <div className="flex flex-wrap gap-2">
                  {resolved.map((persona) => (
                    <div
                      className="flex items-center gap-1.5 rounded-full border border-border/70 bg-muted/30 px-2 py-1"
                      key={persona.id}
                    >
                      <ProfileAvatar
                        avatarUrl={persona.avatarUrl}
                        className="h-5 w-5 rounded-full text-[9px]"
                        label={persona.displayName}
                      />
                      <span className="text-xs font-medium">
                        {persona.displayName}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            ) : null}

            <div className="space-y-1.5">
              <label className="text-sm font-medium" htmlFor="team-channel-id">
                Channel
              </label>
              <select
                className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-2 text-sm shadow-sm"
                disabled={channels.length === 0 || deployMutation.isPending}
                id="team-channel-id"
                onChange={(event) => setChannelId(event.target.value)}
                value={channelId}
              >
                {channels.length === 0 ? (
                  <option value="">No channels available</option>
                ) : null}
                {channels.map((channel) => (
                  <option key={channel.id} value={channel.id}>
                    {channel.name} · {channel.visibility}
                  </option>
                ))}
              </select>
            </div>

            <div className="space-y-1.5">
              <label
                className="text-sm font-medium"
                htmlFor="team-channel-role"
              >
                Role
              </label>
              <select
                className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-2 text-sm shadow-sm"
                disabled={deployMutation.isPending}
                id="team-channel-role"
                onChange={(event) =>
                  setRole(event.target.value as Exclude<ChannelRole, "owner">)
                }
                value={role}
              >
                <option value="bot">bot</option>
                <option value="member">member</option>
                <option value="guest">guest</option>
                <option value="admin">admin</option>
              </select>
            </div>

            {!defaultProvider && !providersQuery.isLoading ? (
              <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                No ACP providers found. Make sure an agent runtime (e.g. Goose)
                is installed.
              </p>
            ) : null}

            {providerWarnings.length > 0
              ? providerWarnings.map((warning) => (
                  <div
                    className="flex gap-3 rounded-2xl border border-amber-400/60 bg-amber-50/60 px-4 py-3 dark:border-amber-500/40 dark:bg-amber-950/30"
                    key={warning}
                  >
                    <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-600 dark:text-amber-400" />
                    <p className="text-sm text-amber-800 dark:text-amber-300">
                      {warning}
                    </p>
                  </div>
                ))
              : null}

            {channelsQuery.error instanceof Error ? (
              <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {channelsQuery.error.message}
              </p>
            ) : null}

            {deployMutation.error instanceof Error ? (
              <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {deployMutation.error.message}
              </p>
            ) : null}
          </div>

          <div className="flex justify-end gap-2 border-t border-border/60 px-6 py-4">
            <Button
              onClick={() => handleOpenChange(false)}
              size="sm"
              type="button"
              variant="outline"
            >
              Cancel
            </Button>
            <Button
              disabled={
                !team ||
                !selectedChannel ||
                !defaultProvider ||
                resolved.length === 0 ||
                channelsQuery.isLoading ||
                providersQuery.isLoading ||
                deployMutation.isPending
              }
              onClick={() => void handleDeploy()}
              size="sm"
              type="button"
            >
              {deployMutation.isPending
                ? "Deploying..."
                : `Deploy ${resolved.length} ${resolved.length === 1 ? "agent" : "agents"}`}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
