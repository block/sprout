import type { Channel } from "@/shared/api/types";

const relativeTimeFormatter = new Intl.RelativeTimeFormat("en-US", {
  numeric: "auto",
});
const absoluteTimeFormatter = new Intl.DateTimeFormat("en-US", {
  month: "short",
  day: "numeric",
  hour: "numeric",
  minute: "2-digit",
});

type EphemeralChannelLike = Pick<Channel, "ttlSeconds" | "ttlDeadline">;

export const EPHEMERAL_CHANNEL_LABEL = "Ephemeral";

export type EphemeralChannelDisplay = {
  detailLabel: string | null;
  tooltipLabel: string;
};

export function isEphemeralChannel(channel: EphemeralChannelLike): boolean {
  return channel.ttlSeconds !== null || channel.ttlDeadline !== null;
}

function resolveRemainingSeconds(
  ttlDeadline: string | null,
  nowMs: number,
): number | null {
  if (!ttlDeadline) {
    return null;
  }

  const deadlineMs = Date.parse(ttlDeadline);
  if (Number.isNaN(deadlineMs)) {
    return null;
  }

  return Math.ceil((deadlineMs - nowMs) / 1_000);
}

function formatCompactRemaining(remainingSeconds: number): string {
  if (remainingSeconds <= 0) {
    return "Cleanup due";
  }

  if (remainingSeconds <= 60) {
    return "1m left";
  }

  if (remainingSeconds < 60 * 60) {
    return `${Math.max(1, Math.ceil(remainingSeconds / 60))}m left`;
  }

  if (remainingSeconds < 60 * 60 * 24) {
    return `${Math.max(1, Math.ceil(remainingSeconds / (60 * 60)))}h left`;
  }

  return `${Math.max(1, Math.ceil(remainingSeconds / (60 * 60 * 24)))}d left`;
}

function formatVerboseRemaining(remainingSeconds: number): string {
  if (remainingSeconds <= 0) {
    return "now";
  }

  if (remainingSeconds <= 60) {
    return relativeTimeFormatter.format(1, "minute");
  }

  if (remainingSeconds < 60 * 60) {
    return relativeTimeFormatter.format(
      Math.max(1, Math.ceil(remainingSeconds / 60)),
      "minute",
    );
  }

  if (remainingSeconds < 60 * 60 * 24) {
    return relativeTimeFormatter.format(
      Math.max(1, Math.ceil(remainingSeconds / (60 * 60))),
      "hour",
    );
  }

  return relativeTimeFormatter.format(
    Math.max(1, Math.ceil(remainingSeconds / (60 * 60 * 24))),
    "day",
  );
}

function formatCompactTtl(ttlSeconds: number): string {
  if (ttlSeconds < 60) {
    return `${Math.max(1, ttlSeconds)}s TTL`;
  }

  if (ttlSeconds < 60 * 60) {
    return `${Math.max(1, Math.ceil(ttlSeconds / 60))}m TTL`;
  }

  if (ttlSeconds < 60 * 60 * 24) {
    return `${Math.max(1, Math.ceil(ttlSeconds / (60 * 60)))}h TTL`;
  }

  return `${Math.max(1, Math.ceil(ttlSeconds / (60 * 60 * 24)))}d TTL`;
}

function formatVerboseTtl(ttlSeconds: number): string {
  if (ttlSeconds < 60) {
    const seconds = Math.max(1, ttlSeconds);
    return `${seconds} second${seconds === 1 ? "" : "s"}`;
  }

  if (ttlSeconds < 60 * 60) {
    const minutes = Math.max(1, Math.ceil(ttlSeconds / 60));
    return `${minutes} minute${minutes === 1 ? "" : "s"}`;
  }

  if (ttlSeconds < 60 * 60 * 24) {
    const hours = Math.max(1, Math.ceil(ttlSeconds / (60 * 60)));
    return `${hours} hour${hours === 1 ? "" : "s"}`;
  }

  const days = Math.max(1, Math.ceil(ttlSeconds / (60 * 60 * 24)));
  return `${days} day${days === 1 ? "" : "s"}`;
}

export function getEphemeralChannelDisplay(
  channel: EphemeralChannelLike,
  nowMs = Date.now(),
): EphemeralChannelDisplay | null {
  if (!isEphemeralChannel(channel)) {
    return null;
  }

  const remainingSeconds = resolveRemainingSeconds(channel.ttlDeadline, nowMs);
  const absoluteDeadlineLabel =
    channel.ttlDeadline && !Number.isNaN(Date.parse(channel.ttlDeadline))
      ? absoluteTimeFormatter.format(new Date(channel.ttlDeadline))
      : null;
  if (remainingSeconds === null) {
    return {
      detailLabel:
        channel.ttlSeconds === null
          ? null
          : formatCompactTtl(channel.ttlSeconds),
      tooltipLabel:
        channel.ttlSeconds === null
          ? "Ephemeral channel. Cleans up automatically after inactivity."
          : `Ephemeral channel. Cleans up after ${formatVerboseTtl(
              channel.ttlSeconds,
            )} of inactivity.`,
    };
  }

  const compactRemaining = formatCompactRemaining(remainingSeconds);
  const verboseRemaining = formatVerboseRemaining(remainingSeconds);

  return {
    detailLabel: compactRemaining,
    tooltipLabel:
      compactRemaining === "Cleanup due"
        ? "Ephemeral channel. Cleanup is due now."
        : absoluteDeadlineLabel
          ? `Ephemeral channel. Cleans up ${verboseRemaining}. Scheduled for ${absoluteDeadlineLabel}.`
          : `Ephemeral channel. Cleans up ${verboseRemaining}.`,
  };
}
