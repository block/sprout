import type { TokenScope } from "@/shared/api/types";

export const TOKEN_SCOPE_OPTIONS: Array<{
  value: TokenScope;
  label: string;
  description: string;
}> = [
  {
    value: "messages:read",
    label: "Messages: Read",
    description: "Read messages in joined channels",
  },
  {
    value: "messages:write",
    label: "Messages: Write",
    description: "Send messages and replies",
  },
  {
    value: "channels:read",
    label: "Channels: Read",
    description: "List and view channel metadata",
  },
  {
    value: "channels:write",
    label: "Channels: Write",
    description: "Create and update channels",
  },
  {
    value: "users:read",
    label: "Users: Read",
    description: "View user profiles and presence",
  },
  {
    value: "files:read",
    label: "Files: Read",
    description: "Download uploaded files",
  },
  {
    value: "files:write",
    label: "Files: Write",
    description: "Upload files to channels",
  },
];

export const MANAGED_AGENT_SCOPE_OPTIONS: Array<{
  value: TokenScope;
  label: string;
  description: string;
}> = [...TOKEN_SCOPE_OPTIONS];

export const DEFAULT_MANAGED_AGENT_SCOPES: TokenScope[] = [
  "messages:read",
  "messages:write",
  "channels:read",
  "users:read",
];
