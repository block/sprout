import { invokeTauri } from "@/shared/api/tauri";

import { PERSONA_CREATOR_SYSTEM_PROMPT } from "./prompt";

type ChatResponse = {
  content: string;
};

/**
 * Send conversation messages to the LLM for the persona creator.
 * Calls the `persona_creator_chat` Tauri command which handles
 * API key resolution and provider selection.
 */
export async function personaCreatorChat(
  messages: ReadonlyArray<{ role: string; content: string }>,
): Promise<string> {
  const response = await invokeTauri<ChatResponse>("persona_creator_chat", {
    systemPrompt: PERSONA_CREATOR_SYSTEM_PROMPT,
    messages: messages.map((m) => ({ role: m.role, content: m.content })),
  });
  return response.content;
}
