import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, "..");

const rules = [
  {
    root: "src-tauri/src",
    extensions: new Set([".rs"]),
    maxLines: 500,
  },
  {
    root: "src/app",
    extensions: new Set([".ts", ".tsx"]),
    maxLines: 500,
  },
  {
    root: "src/features",
    extensions: new Set([".ts", ".tsx"]),
    maxLines: 500,
  },
  {
    root: "src/shared/api",
    extensions: new Set([".ts", ".tsx"]),
    maxLines: 500,
  },
];

// Exceptions should stay rare and temporary. Prefer splitting files instead.
const overrides = new Map([
  ["src-tauri/src/managed_agents/persona_card.rs", 700], // PNG/ZIP persona card codec + 21 unit tests (~300 lines of tests)
  ["src/app/AppShell.tsx", 775],
  ["src/features/channels/hooks.ts", 550], // canvas query + mutation hooks + DM hide mutation
  ["src/features/channels/ui/ChannelManagementSheet.tsx", 800],
  ["src/features/messages/ui/MessageComposer.tsx", 665], // media upload handlers (paste, drop, dialog) + channelId reset effect
  ["src/features/settings/ui/SettingsView.tsx", 600],
  ["src/features/sidebar/ui/AppSidebar.tsx", 850], // channels + forums creation forms
  ["src/features/tokens/ui/TokenSettingsCard.tsx", 800],
  ["src/shared/api/relayClientSession.ts", 725], // durable websocket session manager with reconnect/replay/recovery state
  ["src/shared/api/tauri.ts", 1100], // remote agent provider API bindings + canvas API functions
  ["src-tauri/src/commands/agents.rs", 820], // remote agent lifecycle routing (local + provider branches) + scope enforcement
  ["src-tauri/src/managed_agents/backend.rs", 530], // provider IPC, validation, discovery, binary resolution + tests
  ["src/features/agents/ui/AgentsView.tsx", 775], // remote agent stop/delete + channel UUID resolution + presence-aware delete guard + persona/team import
  ["src/features/agents/ui/CreateAgentDialog.tsx", 685], // provider selector + config form + schema-typed config coercion + required field validation + locked scopes
  ["src/features/channels/ui/AddChannelBotDialog.tsx", 600], // provider mode: Run on selector, trust warning, probe effect, single-agent enforcement
]);

async function walkFiles(directory) {
  const entries = await fs.readdir(directory, { withFileTypes: true });
  const files = await Promise.all(
    entries.map(async (entry) => {
      const fullPath = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        return walkFiles(fullPath);
      }

      return [fullPath];
    }),
  );

  return files.flat();
}

function findRule(relativePath) {
  return rules.find((rule) => {
    const normalizedRoot = `${rule.root}${path.sep}`;
    return relativePath.startsWith(normalizedRoot);
  });
}

function countLines(content) {
  if (content.length === 0) {
    return 0;
  }

  return content.split(/\r?\n/).length;
}

const candidateFiles = (
  await Promise.all(
    rules.map((rule) => walkFiles(path.join(projectRoot, rule.root))),
  )
).flat();

const violations = [];

for (const filePath of candidateFiles) {
  const relativePath = path.relative(projectRoot, filePath);
  const rule = findRule(relativePath);
  if (!rule) {
    continue;
  }

  const extension = path.extname(relativePath);
  if (!rule.extensions.has(extension)) {
    continue;
  }

  const limit = overrides.get(relativePath) ?? rule.maxLines;
  const content = await fs.readFile(filePath, "utf8");
  const lineCount = countLines(content);
  if (lineCount > limit) {
    violations.push({
      limit,
      lineCount,
      relativePath,
    });
  }
}

if (violations.length > 0) {
  console.error("Desktop file size check failed:");
  for (const violation of violations) {
    console.error(
      `- ${violation.relativePath}: ${violation.lineCount} lines (limit ${violation.limit})`,
    );
  }
  console.error(
    "Split the file or add a narrowly scoped exception in `desktop/scripts/check-file-sizes.mjs`.",
  );
  process.exit(1);
}
