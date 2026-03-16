import type { TokenScope } from "@/shared/api/types";

export const AGENT_SCOPE_OPTIONS: Array<{ value: TokenScope; label: string }> =
  [
    { value: "messages:read", label: "Messages read" },
    { value: "messages:write", label: "Messages write" },
    { value: "channels:read", label: "Channels read" },
    { value: "channels:write", label: "Channels write" },
    { value: "users:read", label: "Users read" },
    { value: "users:write", label: "Users write" },
    { value: "files:read", label: "Files read" },
    { value: "files:write", label: "Files write" },
  ];

export function formatTimestamp(value: string | null) {
  if (!value) {
    return "Never";
  }

  return new Intl.DateTimeFormat("en-US", {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(value));
}

export function truncatePubkey(pubkey: string) {
  return `${pubkey.slice(0, 8)}…${pubkey.slice(-6)}`;
}

function commandLooksLikePath(command: string) {
  const trimmed = command.trim();
  return (
    trimmed.startsWith(".") ||
    trimmed.startsWith("~") ||
    trimmed.includes("/") ||
    trimmed.includes("\\")
  );
}

export function describeResolvedCommand(command: string, resolvedPath: string) {
  const normalized = resolvedPath.replace(/\\/g, "/");

  if (normalized.includes("/target/release/")) {
    return "workspace release build";
  }
  if (normalized.includes("/target/debug/")) {
    return "workspace debug build";
  }

  if (commandLooksLikePath(command)) {
    return "custom command";
  }

  return "installed on PATH";
}

export function describeLogFile(path: string) {
  const normalized = path.replace(/\\/g, "/");
  const basename = normalized.split("/").pop() ?? path;

  if (!basename.endsWith(".log")) {
    return "local harness log";
  }

  const stem = basename.slice(0, -4);
  if (stem.length <= 18) {
    return basename;
  }

  return `${stem.slice(0, 8)}…${stem.slice(-6)}.log`;
}
