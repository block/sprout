import * as React from "react";
import { ArrowUpRight, ChevronDown, Wrench } from "lucide-react";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import type { Channel } from "@/shared/api/types";
import { useChannelNavigation } from "@/shared/context/ChannelNavigationContext";
import { cn } from "@/shared/lib/cn";
import type { TranscriptItem } from "./agentSessionTypes";
import {
  formatToolTitle,
  getSproutToolInfo,
  getToolStatusDisplay,
} from "./agentSessionToolCatalog";
import {
  asRecord,
  formatCodeValue,
  getResultArray,
  getToolString,
  getToolStringList,
  shortenMiddle,
} from "./agentSessionUtils";

export function ToolItem({
  enableInlineNavigation = true,
  item,
}: {
  enableInlineNavigation?: boolean;
  item: Extract<TranscriptItem, { type: "tool" }>;
}) {
  const [isExpanded, setIsExpanded] = React.useState(false);
  const status = getToolStatusDisplay(item.status, item.isError);
  const hasArgs = Object.keys(item.args).length > 0;
  const hasResult = item.result.trim().length > 0;
  const canonicalToolName = item.sproutToolName ?? item.toolName;
  const sproutTool = getSproutToolInfo(canonicalToolName);
  const ToolIcon = sproutTool?.icon ?? Wrench;
  const showStatus = status.state !== "output-available";
  const toolTitle = formatToolTitle(canonicalToolName, item.title);
  const handleToggle = React.useCallback(
    (event: React.SyntheticEvent<HTMLDetailsElement>) => {
      setIsExpanded(event.currentTarget.open);
    },
    [],
  );

  return (
    <div className="not-prose w-full px-0">
      <details
        className="group w-full"
        onToggle={handleToggle}
        open={isExpanded}
      >
        <summary className="inline-flex max-w-full cursor-pointer list-none items-center gap-1.5 py-0.5 text-left">
          {ToolIcon ? (
            <ToolIcon
              className={cn(
                "h-4 w-4 shrink-0",
                sproutTool ? "text-primary" : "text-muted-foreground",
              )}
            />
          ) : null}
          <span className="min-w-0 truncate text-sm font-medium">
            {toolTitle}
          </span>
          {sproutTool && enableInlineNavigation ? (
            <SproutToolInlineAction args={item.args} result={item.result} />
          ) : null}
          {showStatus ? (
            <span className="flex shrink-0 items-center gap-1 text-xs text-muted-foreground">
              <status.Icon
                className={cn(
                  "h-4 w-4",
                  item.status === "executing" && "animate-pulse",
                )}
              />
              {status.label}
            </span>
          ) : null}
          <ChevronDown className="h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform group-open:rotate-180" />
        </summary>

        <ToolDetailBlocks
          args={item.args}
          description={sproutTool?.label}
          hasArgs={hasArgs}
          hasResult={hasResult}
          isError={item.isError}
          result={item.result}
        />
      </details>
    </div>
  );
}

function ToolDetailBlocks({
  args,
  description,
  hasArgs,
  hasResult,
  isError,
  result,
}: {
  args: Record<string, unknown>;
  description?: string;
  hasArgs: boolean;
  hasResult: boolean;
  isError: boolean;
  result: string;
}) {
  return (
    <div className="space-y-4 py-2 pl-5 text-popover-foreground outline-none">
      {description ? (
        <p className="max-w-2xl text-xs leading-5 text-muted-foreground">
          {description}
        </p>
      ) : null}
      {hasArgs ? (
        <ToolCodeBlock
          label="Parameters"
          tone="muted"
          value={JSON.stringify(args, null, 2)}
        />
      ) : null}
      {hasResult ? (
        <ToolCodeBlock
          label={isError ? "Error" : "Result"}
          tone={isError ? "error" : "muted"}
          value={result}
        />
      ) : null}
      {!hasArgs && !hasResult ? (
        <p className="text-sm text-muted-foreground/80">
          Waiting for tool details.
        </p>
      ) : null}
    </div>
  );
}

function ToolCodeBlock({
  label,
  tone,
  value,
}: {
  label: string;
  tone: "muted" | "error";
  value: string;
}) {
  return (
    <div className="space-y-2 overflow-hidden">
      <h4 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
        {label}
      </h4>
      <pre
        className={cn(
          "max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-md px-3 py-2 font-mono text-xs leading-5",
          tone === "error"
            ? "bg-destructive/10 text-destructive"
            : "bg-muted/50 text-foreground",
        )}
      >
        {formatCodeValue(value)}
      </pre>
    </div>
  );
}

function SproutToolInlineAction({
  args,
  result,
}: {
  args: Record<string, unknown>;
  result: string;
}) {
  const { channels } = useChannelNavigation();
  const { goChannel } = useAppNavigation();
  const resultValue = React.useMemo(
    () => parseToolResultValue(result),
    [result],
  );
  const resultRecord = asRecord(resultValue);
  const channelId =
    getToolString(args, ["channel_id", "channelId"]) ??
    getToolString(resultRecord, ["channel_id", "channelId"]);
  const openChannel = React.useCallback(
    (messageId?: string) => {
      if (!channelId) return;
      void goChannel(channelId, messageId ? { messageId } : undefined);
    },
    [channelId, goChannel],
  );
  const action = React.useMemo(
    () =>
      getSproutToolInlineAction({
        args,
        channelId,
        channels,
        openChannel,
        resultValue,
      }),
    [args, channelId, channels, openChannel, resultValue],
  );

  if (!action) {
    return null;
  }

  if (action.onClick) {
    return (
      <button
        className="inline-flex max-w-[14rem] shrink min-w-0 items-center gap-1 rounded-full border border-primary/20 bg-primary/[0.05] px-1.5 py-0.5 text-[11px] font-normal leading-none text-primary/90 transition-colors hover:border-primary/35 hover:bg-primary/10 hover:text-primary"
        onClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          action.onClick?.();
        }}
        title={action.title}
        type="button"
      >
        <span className="shrink-0">{action.label}</span>
        <span className="truncate">{action.value}</span>
        <ArrowUpRight className="h-3 w-3 shrink-0" />
      </button>
    );
  }

  return (
    <span
      className="inline-flex max-w-[14rem] shrink min-w-0 items-center gap-1 rounded-full border border-border/60 bg-muted/40 px-1.5 py-0.5 text-[11px] font-normal leading-none text-muted-foreground"
      title={action.title}
    >
      <span className="shrink-0">{action.label}</span>
      <span className="truncate">{action.value}</span>
    </span>
  );
}

type SproutToolInlineActionModel = {
  label: string;
  value: string;
  title: string;
  onClick?: () => void;
};

function getSproutToolInlineAction({
  args,
  channelId,
  channels,
  openChannel,
  resultValue,
}: {
  args: Record<string, unknown>;
  channelId: string | null;
  channels: Channel[];
  openChannel: (messageId?: string) => void;
  resultValue: unknown;
}): SproutToolInlineActionModel | null {
  const resultRecord = asRecord(resultValue);
  const eventId =
    getToolString(args, ["event_id", "eventId"]) ??
    getToolString(resultRecord, ["event_id", "eventId", "id"]);

  if (eventId && channelId) {
    return {
      label: resultRecord.accepted === true ? "posted" : "event",
      onClick: () => openChannel(eventId),
      title: eventId,
      value: getChannelChipLabel(channels, channelId),
    };
  }

  const messages = getResultArray(resultValue, resultRecord, "messages");
  if (messages) {
    return {
      label: "read",
      onClick: channelId ? () => openChannel() : undefined,
      title: `${messages.length} messages`,
      value: `${messages.length} message${messages.length === 1 ? "" : "s"}`,
    };
  }

  if (channelId) {
    return {
      label: "channel",
      onClick: () => openChannel(),
      title: channelId,
      value: getChannelChipLabel(channels, channelId),
    };
  }

  const workflowId =
    getToolString(args, ["workflow_id", "workflowId"]) ??
    getToolString(resultRecord, ["workflow_id", "workflowId"]);
  if (workflowId) {
    return {
      label: "workflow",
      title: workflowId,
      value: shortenMiddle(workflowId, 26),
    };
  }

  const pubkeys = getToolStringList(args, ["pubkeys", "pubkey"]);
  if (pubkeys.length > 0) {
    return {
      label: pubkeys.length === 1 ? "pubkey" : "users",
      title: pubkeys.join(", "),
      value:
        pubkeys.length === 1
          ? shortenMiddle(pubkeys[0], 24)
          : `${pubkeys.length} pubkeys`,
    };
  }

  const query = getToolString(args, ["query"]);
  if (query) {
    return {
      label: "query",
      title: query,
      value: shortenMiddle(query, 30),
    };
  }

  if (typeof resultRecord.accepted === "boolean") {
    return {
      label: "relay",
      title: resultRecord.accepted ? "accepted" : "rejected",
      value: resultRecord.accepted ? "accepted" : "rejected",
    };
  }

  return null;
}

function parseToolResultValue(result: string): unknown {
  const trimmed = result.trim();
  if (!trimmed) return null;

  try {
    const parsed = JSON.parse(trimmed);
    if (typeof parsed !== "string") return parsed;
    try {
      return JSON.parse(parsed);
    } catch {
      return parsed;
    }
  } catch {
    return null;
  }
}

function getChannelChipLabel(channels: Channel[], channelId: string) {
  const channel = channels.find((candidate) => candidate.id === channelId);
  return channel ? `#${channel.name}` : `#${shortenMiddle(channelId, 22)}`;
}
