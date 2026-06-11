import {
  addChannelMembers,
  createManagedAgent,
  getChannelMembers,
  listManagedAgents,
} from "@/shared/api/tauri";
import { sendManagedAgentChannelMessage } from "@/shared/api/tauriManagedAgentMessages";
import { listPersonas, setPersonaActive } from "@/shared/api/tauriPersonas";
import type { ManagedAgent } from "@/shared/api/types";
import { normalizePubkey } from "@/shared/lib/pubkey";

export const WELCOME_GUIDE_AGENT_NAME = "Kit";
export const WELCOME_GUIDE_PERSONA_ID = "builtin:kit";
export const WELCOME_GUIDE_INTRO_MARKER = "sprout-welcome-intro.v1";
const LEGACY_WELCOME_GUIDE_SYSTEM_PROMPT =
  "You are Kit, Sprout's friendly welcome guide. Help new users understand the workspace, channels, messages, and agents. Keep introductions concise, practical, and warm.";
export const WELCOME_GUIDE_INTRO_MESSAGE =
  "Hi, I'm Kit. Welcome to Sprout.\n\nI can help you get oriented, answer questions, and make the first few steps feel less mysterious.\n\nFeel free to ask me what else you can do in Sprout, or just talk through what you want to build.";

function isNamedKitAgent(agent: ManagedAgent) {
  return (
    agent.name.trim().toLowerCase() === WELCOME_GUIDE_AGENT_NAME.toLowerCase()
  );
}

function isBuiltInWelcomeGuideAgent(agent: ManagedAgent) {
  return agent.personaId === WELCOME_GUIDE_PERSONA_ID && isNamedKitAgent(agent);
}

function isLegacyWelcomeGuideAgent(agent: ManagedAgent) {
  return (
    isNamedKitAgent(agent) &&
    agent.systemPrompt?.trim() === LEGACY_WELCOME_GUIDE_SYSTEM_PROMPT
  );
}

function isWelcomeGuideAgent(agent: ManagedAgent) {
  return isBuiltInWelcomeGuideAgent(agent) || isLegacyWelcomeGuideAgent(agent);
}

function pickAgentByStatus(agents: ManagedAgent[]) {
  return (
    agents.find((agent) => agent.status === "running") ??
    agents.find((agent) => agent.status === "deployed") ??
    agents[0] ??
    null
  );
}

export async function getWelcomeGuideAgentPubkeys() {
  return (await listManagedAgents())
    .filter(isWelcomeGuideAgent)
    .map((agent) => agent.pubkey);
}

async function ensureWelcomeGuidePersonaActive() {
  const kit = (await listPersonas()).find(
    (persona) => persona.id === WELCOME_GUIDE_PERSONA_ID,
  );
  if (!kit) {
    throw new Error("Kit persona not found.");
  }
  if (!kit.isActive) {
    await setPersonaActive(WELCOME_GUIDE_PERSONA_ID, true);
  }
}

async function ensureWelcomeGuideAgent() {
  const agents = await listManagedAgents();
  const existing = pickAgentByStatus(agents.filter(isBuiltInWelcomeGuideAgent));
  if (existing) {
    return existing;
  }

  await ensureWelcomeGuidePersonaActive();

  const created = await createManagedAgent({
    name: WELCOME_GUIDE_AGENT_NAME,
    personaId: WELCOME_GUIDE_PERSONA_ID,
    spawnAfterCreate: false,
    startOnAppLaunch: false,
    respondTo: "owner-only",
  });

  return created.agent;
}

async function ensureWelcomeGuideMembership(
  channelId: string,
  agent: ManagedAgent,
) {
  const agentPubkey = normalizePubkey(agent.pubkey);
  const members = await getChannelMembers(channelId).catch(() => []);
  if (
    members.some((member) => normalizePubkey(member.pubkey) === agentPubkey)
  ) {
    return;
  }

  const result = await addChannelMembers({
    channelId,
    pubkeys: [agent.pubkey],
    role: "bot",
  });
  const error = result.errors.find(
    (entry) => normalizePubkey(entry.pubkey) === agentPubkey,
  );
  if (error && !error.error.toLowerCase().includes("already")) {
    throw new Error(error.error);
  }
}

export async function ensureWelcomeGuideIntro(channelId: string) {
  const agent = await ensureWelcomeGuideAgent();
  await ensureWelcomeGuideMembership(channelId, agent);
  await sendManagedAgentChannelMessage({
    agentPubkey: agent.pubkey,
    channelId,
    content: WELCOME_GUIDE_INTRO_MESSAGE,
    marker: WELCOME_GUIDE_INTRO_MARKER,
  });
  return agent;
}
