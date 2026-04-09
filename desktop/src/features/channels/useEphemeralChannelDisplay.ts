import * as React from "react";

import {
  getEphemeralChannelDisplay,
  isEphemeralChannel,
} from "@/features/channels/lib/ephemeralChannel";
import type { Channel } from "@/shared/api/types";

export function useEphemeralChannelDisplay(channel: Channel | null) {
  const [ephemeralClock, setEphemeralClock] = React.useState(() => Date.now());
  const hasEphemeralDeadline =
    channel !== null &&
    isEphemeralChannel(channel) &&
    channel.ttlDeadline !== null;

  React.useEffect(() => {
    if (!hasEphemeralDeadline) {
      return;
    }

    setEphemeralClock(Date.now());
    const intervalId = window.setInterval(() => {
      setEphemeralClock(Date.now());
    }, 60_000);

    return () => {
      window.clearInterval(intervalId);
    };
  }, [hasEphemeralDeadline]);

  return channel ? getEphemeralChannelDisplay(channel, ephemeralClock) : null;
}
