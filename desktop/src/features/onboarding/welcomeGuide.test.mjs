import assert from "node:assert/strict";
import test from "node:test";

import {
  LEGACY_WELCOME_GUIDE_SYSTEM_PROMPT,
  pickWelcomeGuideAgent,
  pickWelcomeGuideAgentForRelay,
  WELCOME_GUIDE_AGENT_NAME,
  WELCOME_GUIDE_PERSONA_ID,
} from "./welcomeGuide.ts";

const PUB_A = "a".repeat(64);
const PUB_B = "b".repeat(64);
const PUB_C = "c".repeat(64);
const RELAY_A = "ws://localhost:3000";
const RELAY_B = "ws://localhost:3001";

function makeAgent(overrides = {}) {
  return {
    pubkey: PUB_A,
    name: WELCOME_GUIDE_AGENT_NAME,
    personaId: null,
    relayUrl: RELAY_A,
    acpCommand: "sprout-acp",
    agentCommand: "sprout-agent",
    agentArgs: [],
    mcpCommand: "sprout-dev-mcp",
    turnTimeoutSeconds: 120,
    idleTimeoutSeconds: null,
    maxTurnDurationSeconds: null,
    parallelism: 1,
    systemPrompt: null,
    model: null,
    mcpToolsets: null,
    envVars: {},
    status: "stopped",
    pid: null,
    createdAt: "2026-06-11T00:00:00.000Z",
    updatedAt: "2026-06-11T00:00:00.000Z",
    lastStartedAt: null,
    lastStoppedAt: null,
    lastExitCode: null,
    lastError: null,
    logPath: "",
    startOnAppLaunch: false,
    backend: { type: "local" },
    backendAgentId: null,
    respondTo: "owner-only",
    respondToAllowlist: [],
    ...overrides,
  };
}

test("pickWelcomeGuideAgent reuses a legacy Kit guide", () => {
  const legacyKit = makeAgent({
    pubkey: PUB_A,
    systemPrompt: LEGACY_WELCOME_GUIDE_SYSTEM_PROMPT,
  });

  assert.equal(pickWelcomeGuideAgent([legacyKit]), legacyKit);
});

test("pickWelcomeGuideAgent prefers a running legacy guide over stopped builtin Kit", () => {
  const stoppedBuiltinKit = makeAgent({
    pubkey: PUB_A,
    personaId: WELCOME_GUIDE_PERSONA_ID,
    status: "stopped",
  });
  const runningLegacyKit = makeAgent({
    pubkey: PUB_B,
    status: "running",
    systemPrompt: LEGACY_WELCOME_GUIDE_SYSTEM_PROMPT,
  });

  assert.equal(
    pickWelcomeGuideAgent([stoppedBuiltinKit, runningLegacyKit]),
    runningLegacyKit,
  );
});

test("pickWelcomeGuideAgent ignores non-Kit agents with the legacy prompt", () => {
  const nonKit = makeAgent({
    pubkey: PUB_A,
    name: "Scout",
    systemPrompt: LEGACY_WELCOME_GUIDE_SYSTEM_PROMPT,
  });
  const kit = makeAgent({
    pubkey: PUB_C,
    personaId: WELCOME_GUIDE_PERSONA_ID,
  });

  assert.equal(pickWelcomeGuideAgent([nonKit, kit]), kit);
});

test("pickWelcomeGuideAgentForRelay ignores Kit agents from other workspaces", () => {
  const otherWorkspaceKit = makeAgent({
    pubkey: PUB_A,
    personaId: WELCOME_GUIDE_PERSONA_ID,
    relayUrl: RELAY_A,
    status: "running",
  });
  const currentWorkspaceKit = makeAgent({
    pubkey: PUB_B,
    personaId: WELCOME_GUIDE_PERSONA_ID,
    relayUrl: RELAY_B,
    status: "stopped",
  });

  assert.equal(
    pickWelcomeGuideAgentForRelay(
      [otherWorkspaceKit, currentWorkspaceKit],
      RELAY_B,
    ),
    currentWorkspaceKit,
  );
});

test("pickWelcomeGuideAgentForRelay returns null when Kit only exists in another workspace", () => {
  const otherWorkspaceKit = makeAgent({
    pubkey: PUB_A,
    personaId: WELCOME_GUIDE_PERSONA_ID,
    relayUrl: RELAY_A,
  });

  assert.equal(
    pickWelcomeGuideAgentForRelay([otherWorkspaceKit], RELAY_B),
    null,
  );
});
