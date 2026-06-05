type OnboardingSoundName =
  | "changeTheme"
  | "minorA"
  | "minorB"
  | "toggleA"
  | "toggleB"
  | "typing"
  | "typingSpace"
  | "uiClick"
  | "toggleMinor"
  | "tada";

type AudioWindow = Window &
  typeof globalThis & {
    webkitAudioContext?: typeof AudioContext;
  };

const SOUND_SOURCES = {
  changeTheme: "/onboarding/sounds/change-theme.wav",
  minorA: "/onboarding/sounds/nav-minor-a.wav",
  minorB: "/onboarding/sounds/nav-minor-b.wav",
  toggleA: "/onboarding/sounds/toggle-a.wav",
  toggleB: "/onboarding/sounds/toggle-b.wav",
  typing: "/onboarding/sounds/typing.wav",
  typingSpace: "/onboarding/sounds/typing-space.wav",
  uiClick: "/onboarding/sounds/navigation-toggle-ui-click.wav",
  toggleMinor: "/onboarding/sounds/toggle-minor-a.wav",
  tada: "/onboarding/sounds/honk-tada.wav",
} satisfies Record<OnboardingSoundName, string>;

const SOUND_VOLUMES = {
  changeTheme: 0.32,
  minorA: 0.3,
  minorB: 0.3,
  toggleA: 0.32,
  toggleB: 0.32,
  typing: 0.18,
  typingSpace: 0.2,
  uiClick: 0.34,
  toggleMinor: 0.3,
  tada: 0.44,
} satisfies Record<OnboardingSoundName, number>;

const fallbackAudioCache = new Map<OnboardingSoundName, HTMLAudioElement>();
const bufferCache = new Map<OnboardingSoundName, AudioBuffer>();
const decodePromises = new Map<
  OnboardingSoundName,
  Promise<AudioBuffer | null>
>();

let audioContext: AudioContext | null = null;
let audioUnlockInstalled = false;

type TypingKeyEvent = Pick<
  KeyboardEvent,
  "altKey" | "ctrlKey" | "key" | "metaKey"
> & {
  isComposing?: boolean;
};

function getAudioContext(): AudioContext | null {
  if (typeof window === "undefined") {
    return null;
  }

  if (audioContext) {
    return audioContext;
  }

  const AudioContextConstructor =
    window.AudioContext ?? (window as AudioWindow).webkitAudioContext;
  if (!AudioContextConstructor) {
    return null;
  }

  audioContext = new AudioContextConstructor();
  return audioContext;
}

function unlockOnboardingAudio() {
  const context = getAudioContext();
  if (!context || context.state !== "suspended") {
    return;
  }

  void context.resume().catch(() => {});
}

function installAudioUnlockListeners() {
  if (typeof window === "undefined" || audioUnlockInstalled) {
    return;
  }

  audioUnlockInstalled = true;
  window.addEventListener("pointerdown", unlockOnboardingAudio, {
    capture: true,
    passive: true,
  });
  window.addEventListener("keydown", unlockOnboardingAudio, { capture: true });
}

function getFallbackAudio(name: OnboardingSoundName): HTMLAudioElement | null {
  if (typeof window === "undefined") {
    return null;
  }

  const cachedAudio = fallbackAudioCache.get(name);
  if (cachedAudio) {
    return cachedAudio;
  }

  const audio = new Audio(SOUND_SOURCES[name]);
  audio.preload = "auto";
  audio.volume = SOUND_VOLUMES[name];
  fallbackAudioCache.set(name, audio);
  return audio;
}

function decodeSound(name: OnboardingSoundName): Promise<AudioBuffer | null> {
  const cachedBuffer = bufferCache.get(name);
  if (cachedBuffer) {
    return Promise.resolve(cachedBuffer);
  }

  const cachedPromise = decodePromises.get(name);
  if (cachedPromise) {
    return cachedPromise;
  }

  const context = getAudioContext();
  if (!context) {
    return Promise.resolve(null);
  }

  const decodePromise = fetch(SOUND_SOURCES[name])
    .then((response) => {
      if (!response.ok) {
        throw new Error(
          `Failed to load onboarding sound: ${SOUND_SOURCES[name]}`,
        );
      }
      return response.arrayBuffer();
    })
    .then((arrayBuffer) => context.decodeAudioData(arrayBuffer))
    .then((audioBuffer) => {
      bufferCache.set(name, audioBuffer);
      return audioBuffer;
    })
    .catch(() => null);

  decodePromises.set(name, decodePromise);
  return decodePromise;
}

function playDecodedSound(
  context: AudioContext,
  name: OnboardingSoundName,
  buffer: AudioBuffer,
) {
  const source = context.createBufferSource();
  const gain = context.createGain();

  source.buffer = buffer;
  gain.gain.value = SOUND_VOLUMES[name];
  source.connect(gain);
  gain.connect(context.destination);
  source.start();
}

function playFallbackSound(name: OnboardingSoundName) {
  const audio = getFallbackAudio(name);
  if (!audio) {
    return;
  }

  audio.currentTime = 0;
  void audio.play().catch(() => {});
}

export function preloadOnboardingSounds() {
  installAudioUnlockListeners();

  for (const soundName of Object.keys(SOUND_SOURCES) as OnboardingSoundName[]) {
    void decodeSound(soundName);
    getFallbackAudio(soundName)?.load();
  }
}

export function playOnboardingSound(name: OnboardingSoundName) {
  installAudioUnlockListeners();

  const context = getAudioContext();
  const buffer = bufferCache.get(name);
  if (context && buffer) {
    unlockOnboardingAudio();
    playDecodedSound(context, name, buffer);
    return;
  }

  void decodeSound(name);
  playFallbackSound(name);
}

export function playOnboardingTypingSoundForKey(event: TypingKeyEvent) {
  if (event.altKey || event.ctrlKey || event.metaKey || event.isComposing) {
    return;
  }

  if (event.key === " " || event.key === "Spacebar") {
    playOnboardingSound("typingSpace");
    return;
  }

  if (event.key.length === 1) {
    playOnboardingSound("typing");
  }
}
