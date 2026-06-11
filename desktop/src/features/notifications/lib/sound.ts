import {
  KIND_APPROVAL_REQUEST,
  KIND_JOB_ACCEPTED,
  KIND_JOB_ERROR,
  KIND_JOB_PROGRESS,
  KIND_JOB_RESULT,
} from "@/shared/constants/kinds";
import type { FeedItemCategory } from "@/shared/api/types";

export const SOUND_NAMES = [
  "bong",
  "boo",
  "dng",
  "doo",
  "doodone",
  "doong",
  "doop",
  "flirl",
  "flutter",
  "oh-no",
  "ping",
  "unison",
] as const;
export type SoundName = (typeof SOUND_NAMES)[number];

export const SOUND_SLOTS = [
  "dm",
  "mention",
  "needs_action",
  "job_accepted",
  "job_progress",
  "job_result",
  "job_error",
] as const;
export type SoundSlot = (typeof SOUND_SLOTS)[number];

export const SLOT_LABELS: Record<SoundSlot, string> = {
  dm: "Direct messages",
  mention: "@Mentions",
  needs_action: "Needs action",
  job_accepted: "Agent: job accepted",
  job_progress: "Agent: progress update",
  job_result: "Agent: job result",
  job_error: "Agent: job error",
};

export const RECOMMENDED_SOUND_BY_SLOT: Record<SoundSlot, SoundName> = {
  dm: "unison",
  mention: "ping",
  needs_action: "doodone",
  job_accepted: "boo",
  job_progress: "dng",
  job_result: "doop",
  job_error: "oh-no",
};

export const SOUND_MODES = ["single", "custom"] as const;
export type SoundMode = (typeof SOUND_MODES)[number];

export const RECOMMENDED_SINGLE_SOUND: SoundName = "unison";

export type SoundPreferences = {
  soundMode: SoundMode;
  singleSound: SoundName;
  sounds: Record<SoundSlot, SoundName>;
};

export function resolveSlotSound(
  prefs: SoundPreferences,
  slot: SoundSlot,
): SoundName {
  return prefs.soundMode === "custom" ? prefs.sounds[slot] : prefs.singleSound;
}

export function slotForFeedKind(
  kind: number,
  category: FeedItemCategory,
): SoundSlot {
  if (category === "mention") return "mention";
  if (kind === KIND_JOB_ACCEPTED) return "job_accepted";
  if (kind === KIND_JOB_PROGRESS) return "job_progress";
  if (kind === KIND_JOB_RESULT) return "job_result";
  if (kind === KIND_JOB_ERROR) return "job_error";
  if (kind === KIND_APPROVAL_REQUEST) return "needs_action";
  return "needs_action";
}

const cache = new Map<SoundName, HTMLAudioElement>();

function getAudio(name: SoundName): HTMLAudioElement {
  let audio = cache.get(name);
  if (!audio) {
    audio = new Audio(`/sounds/${name}.mp3`);
    cache.set(name, audio);
  }
  return audio;
}

export function playNotificationSound(
  name: SoundName,
): HTMLAudioElement | null {
  try {
    const audio = getAudio(name);
    audio.currentTime = 0;
    audio.play().catch(() => {
      // Best-effort — user may not have interacted with the page yet.
    });
    return audio;
  } catch {
    // Best-effort only.
    return null;
  }
}
