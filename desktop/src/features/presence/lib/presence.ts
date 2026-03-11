import type { PresenceStatus } from "@/shared/api/types";

export function getPresenceLabel(status: PresenceStatus) {
  switch (status) {
    case "online":
      return "Online";
    case "away":
      return "Away";
    case "offline":
      return "Offline";
  }
}

export function getPresenceDotClassName(status: PresenceStatus) {
  switch (status) {
    case "online":
      return "bg-emerald-500";
    case "away":
      return "bg-amber-500";
    case "offline":
      return "bg-muted-foreground/35";
  }
}
