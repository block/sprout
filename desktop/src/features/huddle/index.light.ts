/**
 * No-op huddle module — selected via Vite alias when VITE_SPROUT_HUDDLE=0.
 *
 * Mirrors the public API of ./index.ts so consumers (AppShell, ChannelMembersBar)
 * compile and render without changes. Voice/audio code, model downloads, and
 * the Tauri huddle commands are unavailable in this build; this module gives
 * the rest of the UI sensible idle values so it doesn't crash.
 */

import * as React from "react";

type VoiceInputMode = "push_to_talk" | "voice_activity";

interface HuddleContextValue {
  localAudioTrack: MediaStreamTrack | null;
  isStarting: boolean;
  huddleError: string | null;
  clearHuddleError: () => void;
  micConnected: boolean;
  micLevel: number;
  pttActive: boolean;
  voiceInputMode: VoiceInputMode;
  setVoiceInputMode: (mode: VoiceInputMode) => Promise<void>;
  activeSpeakers: string[];
  audioDevices: MediaDeviceInfo[];
  selectedDeviceId: string;
  setSelectedDeviceId: (id: string) => void;
  micGain: number;
  setMicGain: (value: number) => void;
  outputDevices: { name: string; is_default: boolean }[];
  selectedOutputDevice: string;
  setSelectedOutputDevice: (name: string) => void;
  startHuddle: (
    parentChannelId: string,
    memberPubkeys: string[],
  ) => Promise<void>;
  joinHuddle: (
    parentChannelId: string,
    ephemeralChannelId: string,
  ) => Promise<void>;
  leaveHuddle: () => Promise<boolean>;
  endHuddle: () => Promise<boolean>;
}

const DISABLED = "voice/huddle is disabled in this build";

const noopValue: HuddleContextValue = {
  localAudioTrack: null,
  isStarting: false,
  huddleError: null,
  clearHuddleError: () => {},
  micConnected: false,
  micLevel: 0,
  pttActive: false,
  voiceInputMode: "push_to_talk",
  setVoiceInputMode: async () => {},
  activeSpeakers: [],
  audioDevices: [],
  selectedDeviceId: "",
  setSelectedDeviceId: () => {},
  micGain: 1,
  setMicGain: () => {},
  outputDevices: [],
  selectedOutputDevice: "",
  setSelectedOutputDevice: () => {},
  startHuddle: async () => {
    throw new Error(DISABLED);
  },
  joinHuddle: async () => {
    throw new Error(DISABLED);
  },
  leaveHuddle: async () => false,
  endHuddle: async () => false,
};

export function HuddleProvider({ children }: { children: React.ReactNode }) {
  return React.createElement(React.Fragment, null, children);
}

export function useHuddle(): HuddleContextValue {
  return noopValue;
}

export function HuddleBar(): null {
  return null;
}

/** No-op stand-in for the channel-bar headphone icon. */
export function HuddleIndicator(_props: {
  channelId: string;
  className?: string;
  onStart?: () => void;
  startDisabled?: boolean;
}): null {
  return null;
}

export function ParticipantList(_props: {
  ephemeralChannelId: string;
  participants: Set<string>;
}): null {
  return null;
}

/** Audio worklet is unavailable — return a shape compatible with the real one. */
export async function setupAudioWorklet(): Promise<never> {
  throw new Error(DISABLED);
}
