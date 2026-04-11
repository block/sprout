import * as React from "react";

import { getEphemeralChannelDisplay } from "@/features/channels/lib/ephemeralChannel";
import type { Channel } from "@/shared/api/types";

export function useEphemeralChannelDisplay(channel: Channel | null) {
  const [, setClockTick] = React.useState(0);
  const deadlineKey =
    channel?.ttlDeadline === null || channel === null
      ? null
      : `${channel.id}:${channel.ttlDeadline}`;

  React.useEffect(() => {
    if (deadlineKey === null) {
      return;
    }

    const intervalId = window.setInterval(() => {
      setClockTick((current) => current + 1);
    }, 60_000);

    return () => {
      window.clearInterval(intervalId);
    };
  }, [deadlineKey]);

  return channel ? getEphemeralChannelDisplay(channel, Date.now()) : null;
}
