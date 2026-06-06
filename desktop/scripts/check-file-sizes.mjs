import path from "node:path";
import { fileURLToPath } from "node:url";
import { runFileSizeCheck } from "../../scripts/check-file-sizes-core.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, "..");

const MAX_LINES = 1000;

const rules = [
  { root: "src-tauri/src", extensions: new Set([".rs"]), maxLines: MAX_LINES },
  {
    root: "src/app",
    extensions: new Set([".ts", ".tsx"]),
    maxLines: MAX_LINES,
  },
  {
    root: "src/features",
    extensions: new Set([".ts", ".tsx"]),
    maxLines: MAX_LINES,
  },
  {
    root: "src/shared/api",
    extensions: new Set([".ts", ".tsx"]),
    maxLines: MAX_LINES,
  },
];

// TEMP — these files exceed the 1000-line limit and are queued to be split.
// Do not add to this list; split the file instead. Remove each entry as its
// file is broken up. Tracked as a follow-up.
const overrides = new Map([
  ["src-tauri/src/managed_agents/nest.rs", 1415],
  ["src-tauri/src/managed_agents/runtime.rs", 1414], // + SPROUT_SERVERLESS env passthrough + current-workspace relay override
  ["src-tauri/src/huddle/tts.rs", 1364],
  ["src/shared/api/tauri.ts", 1275], // + serverless multi-relay message wrappers + decryptGiftWrap + rootEventId
  ["src-tauri/src/nostr_convert.rs", 1116],
  ["src/shared/api/relayClientSession.ts", 1160], // + serverless mode (setServerless, AUTH-skip) + encrypted channel decrypt
  ["src-tauri/src/migration.rs", 1005],
  ["src-tauri/src/events.rs", 1105], // + serverless addressable builders (39000/39002) + NIP-09 channel delete + self-p-tag guards
]);

await runFileSizeCheck({
  projectRoot,
  rules,
  overrides,
  label: "Desktop",
  scriptPath: "desktop/scripts/check-file-sizes.mjs",
});
