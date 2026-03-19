import type { TokenScope } from "@/shared/api/types";

export const TOKEN_SCOPE_OPTIONS: Array<{
  value: TokenScope;
  label: string;
}> = [
  { value: "messages:read", label: "Messages: Read" },
  { value: "messages:write", label: "Messages: Write" },
  { value: "channels:read", label: "Channels: Read" },
  { value: "channels:write", label: "Channels: Write" },
  { value: "users:read", label: "Users: Read" },
  { value: "files:read", label: "Files: Read" },
  { value: "files:write", label: "Files: Write" },
];

export const MANAGED_AGENT_SCOPE_OPTIONS: Array<{
  value: TokenScope;
  label: string;
}> = [...TOKEN_SCOPE_OPTIONS];

export const DEFAULT_MANAGED_AGENT_SCOPES: TokenScope[] = [
  "messages:read",
  "messages:write",
  "channels:read",
  "users:read",
];
