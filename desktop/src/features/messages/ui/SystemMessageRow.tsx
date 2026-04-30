import { ArrowRightLeft, SmilePlus } from "lucide-react";
import Picker from "@emoji-mart/react";
import data from "@emoji-mart/data";
import * as React from "react";

import type { TimelineMessage } from "@/features/messages/types";
import { MessageReactions } from "@/features/messages/ui/MessageReactions";
import { useReactionHandler } from "@/features/messages/ui/useReactionHandler";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { resolveUserLabel } from "@/features/profile/lib/identity";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { Spinner } from "@/shared/ui/spinner";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";
import { UserAvatar } from "@/shared/ui/UserAvatar";
import { MessageTimestamp } from "./MessageTimestamp";

type SystemMessagePayload = {
  type: string;
  actor?: string;
  target?: string;
  topic?: string;
  purpose?: string;
};

function resolveLabel(
  pubkey: string | undefined,
  currentPubkey: string | undefined,
  profiles: UserProfileLookup | undefined,
): string {
  if (!pubkey) {
    return "Someone";
  }
  return resolveUserLabel({ pubkey, currentPubkey, profiles });
}

function resolvePersonaSuffix(
  pubkey: string | undefined,
  personaLookup: Map<string, string> | undefined,
): string {
  if (!pubkey || !personaLookup) return "";
  const personaName = personaLookup.get(pubkey.toLowerCase());
  return personaName ? ` (${personaName})` : "";
}

function resolveAvatarUrl(
  pubkey: string | undefined,
  profiles: UserProfileLookup | undefined,
): string | null {
  if (!pubkey || !profiles) return null;
  return profiles[pubkey.toLowerCase()]?.avatarUrl ?? null;
}

function UserChip({
  pubkey,
  currentPubkey,
  profiles,
  suffix,
}: {
  pubkey: string | undefined;
  currentPubkey: string | undefined;
  profiles: UserProfileLookup | undefined;
  suffix?: string;
}) {
  const label = resolveLabel(pubkey, currentPubkey, profiles);
  return (
    <span className="inline-flex items-center gap-1">
      <UserAvatar
        avatarUrl={resolveAvatarUrl(pubkey, profiles)}
        displayName={label}
        size="xs"
      />
      <span className="font-medium">
        {label}
        {suffix}
      </span>
    </span>
  );
}

function describeSystemEventStructured(
  payload: SystemMessagePayload,
  currentPubkey: string | undefined,
  profiles: UserProfileLookup | undefined,
  personaLookup?: Map<string, string>,
): React.ReactNode | null {
  const personaSuffix = resolvePersonaSuffix(payload.target, personaLookup);

  switch (payload.type) {
    case "member_joined": {
      if (payload.actor === payload.target) {
        return (
          <span className="inline-flex flex-wrap items-center gap-1">
            <UserChip
              pubkey={payload.actor}
              currentPubkey={currentPubkey}
              profiles={profiles}
              suffix={personaSuffix}
            />
            <span>joined the channel</span>
          </span>
        );
      }
      return (
        <span className="inline-flex flex-wrap items-center gap-1">
          <UserChip
            pubkey={payload.actor}
            currentPubkey={currentPubkey}
            profiles={profiles}
          />
          <span>added</span>
          <UserChip
            pubkey={payload.target}
            currentPubkey={currentPubkey}
            profiles={profiles}
            suffix={personaSuffix}
          />
          <span>to the channel</span>
        </span>
      );
    }
    case "member_left":
      return (
        <span className="inline-flex flex-wrap items-center gap-1">
          <UserChip
            pubkey={payload.actor}
            currentPubkey={currentPubkey}
            profiles={profiles}
          />
          <span>left the channel</span>
        </span>
      );
    case "member_removed":
      return (
        <span className="inline-flex flex-wrap items-center gap-1">
          <UserChip
            pubkey={payload.actor}
            currentPubkey={currentPubkey}
            profiles={profiles}
          />
          <span>removed</span>
          <UserChip
            pubkey={payload.target}
            currentPubkey={currentPubkey}
            profiles={profiles}
          />
          <span>from the channel</span>
        </span>
      );
    case "topic_changed":
      return (
        <span className="inline-flex flex-wrap items-center gap-1">
          <UserChip
            pubkey={payload.actor}
            currentPubkey={currentPubkey}
            profiles={profiles}
          />
          <span>changed the topic to &ldquo;{payload.topic}&rdquo;</span>
        </span>
      );
    case "purpose_changed":
      return (
        <span className="inline-flex flex-wrap items-center gap-1">
          <UserChip
            pubkey={payload.actor}
            currentPubkey={currentPubkey}
            profiles={profiles}
          />
          <span>changed the purpose to &ldquo;{payload.purpose}&rdquo;</span>
        </span>
      );
    case "channel_created":
      return (
        <span className="inline-flex flex-wrap items-center gap-1">
          <UserChip
            pubkey={payload.actor}
            currentPubkey={currentPubkey}
            profiles={profiles}
          />
          <span>created this channel</span>
        </span>
      );
    case "channel_archived":
      return (
        <span className="inline-flex flex-wrap items-center gap-1">
          <UserChip
            pubkey={payload.actor}
            currentPubkey={currentPubkey}
            profiles={profiles}
          />
          <span>archived this channel</span>
        </span>
      );
    case "channel_unarchived":
      return (
        <span className="inline-flex flex-wrap items-center gap-1">
          <UserChip
            pubkey={payload.actor}
            currentPubkey={currentPubkey}
            profiles={profiles}
          />
          <span>unarchived this channel</span>
        </span>
      );
    default:
      return null;
  }
}

export const SystemMessageRow = React.memo(function SystemMessageRow({
  message,
  currentPubkey,
  profiles,
  personaLookup,
  onToggleReaction,
}: {
  message: TimelineMessage;
  currentPubkey?: string;
  profiles?: UserProfileLookup;
  /** Map from lowercase pubkey → persona display name for bot members. */
  personaLookup?: Map<string, string>;
  onToggleReaction?: (
    message: TimelineMessage,
    emoji: string,
    remove: boolean,
  ) => Promise<void>;
}) {
  const [isReactionPickerOpen, setIsReactionPickerOpen] = React.useState(false);
  const {
    reactions,
    canToggle: canToggleReactions,
    pending: reactionPending,
    errorMessage: reactionErrorMessage,
    select: handleReactionSelect,
  } = useReactionHandler(message, onToggleReaction);

  let payload: SystemMessagePayload;
  try {
    payload = JSON.parse(message.body);
  } catch {
    return null;
  }

  const description = describeSystemEventStructured(
    payload,
    currentPubkey,
    profiles,
    personaLookup,
  );
  if (!description) {
    return null;
  }

  return (
    <div
      className="group/message rounded-lg px-2 py-1"
      data-testid="system-message-row"
    >
      <div className="flex items-center gap-2.5">
        <div className="flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-muted">
          <ArrowRightLeft className="h-3 w-3 text-muted-foreground" />
        </div>
        <p className="text-xs text-muted-foreground">{description}</p>
        <div className="ml-auto flex items-center gap-1 text-xs text-muted-foreground/60">
          <div className="relative">
            <div className="absolute right-0 top-1/2 -translate-y-1/2">
              {canToggleReactions ? (
                <div
                  className={cn(
                    "overflow-hidden rounded-full border border-border/70 bg-background/95 shadow-sm backdrop-blur supports-[backdrop-filter]:bg-background/85 transition-all duration-150 ease-out",
                    "max-w-0 border-0 shadow-none translate-y-1 opacity-0",
                    "group-hover/message:max-w-9 group-hover/message:border group-hover/message:border-border/70 group-hover/message:shadow-sm group-hover/message:translate-y-0 group-hover/message:opacity-100",
                    "group-focus-within/message:max-w-9 group-focus-within/message:border group-focus-within/message:border-border/70 group-focus-within/message:shadow-sm group-focus-within/message:translate-y-0 group-focus-within/message:opacity-100",
                    isReactionPickerOpen
                      ? "max-w-9 border border-border/70 shadow-sm translate-y-0 opacity-100"
                      : "",
                  )}
                >
                  <div className="flex items-center gap-1 p-1">
                    <Popover
                      onOpenChange={setIsReactionPickerOpen}
                      open={isReactionPickerOpen}
                    >
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <PopoverTrigger asChild>
                            <Button
                              aria-label="Open reactions"
                              className="h-6 w-6 rounded-full p-0"
                              disabled={reactionPending}
                              size="sm"
                              type="button"
                              variant={
                                isReactionPickerOpen ? "secondary" : "ghost"
                              }
                            >
                              {reactionPending ? (
                                <Spinner className="h-3 w-3" />
                              ) : (
                                <SmilePlus className="h-3 w-3" />
                              )}
                            </Button>
                          </PopoverTrigger>
                        </TooltipTrigger>
                        <TooltipContent>React</TooltipContent>
                      </Tooltip>
                      <PopoverContent
                        align="end"
                        className="w-auto p-0 rounded-2xl overflow-hidden border-0 bg-transparent shadow-none"
                        side="top"
                        sideOffset={10}
                      >
                        {reactionErrorMessage ? (
                          <div className="px-3 pt-3 pb-0">
                            <p className="text-xs text-destructive">
                              {reactionErrorMessage}
                            </p>
                          </div>
                        ) : null}
                        <Picker
                          data={data}
                          onEmojiSelect={(emoji: { native: string }) => {
                            void handleReactionSelect(emoji.native).finally(
                              () => {
                                setIsReactionPickerOpen(false);
                              },
                            );
                          }}
                          theme="auto"
                          previewPosition="none"
                          skinTonePosition="search"
                          set="native"
                          maxFrequentRows={2}
                          perLine={8}
                        />
                      </PopoverContent>
                    </Popover>
                  </div>
                </div>
              ) : null}
            </div>
          </div>
          <MessageTimestamp createdAt={message.createdAt} time={message.time} />
        </div>
      </div>
      <MessageReactions
        messageId={message.id}
        reactions={reactions}
        canToggle={canToggleReactions}
        pending={reactionPending}
        onSelect={(emoji) => {
          void handleReactionSelect(emoji);
        }}
      />
      {reactionErrorMessage ? (
        <p className="mt-1.5 text-xs text-destructive">
          {reactionErrorMessage}
        </p>
      ) : null}
    </div>
  );
});
