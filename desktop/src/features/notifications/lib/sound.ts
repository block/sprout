let cachedAudio: HTMLAudioElement | null = null;

function getNotificationAudio(): HTMLAudioElement {
  if (!cachedAudio) {
    cachedAudio = new Audio("/sounds/desktop-notification.mp3");
  }
  return cachedAudio;
}

export function playNotificationSound(): void {
  try {
    const audio = getNotificationAudio();
    audio.currentTime = 0;
    audio.play().catch(() => {
      // Best-effort — user may not have interacted with the page yet.
    });
  } catch {
    // Best-effort only.
  }
}
