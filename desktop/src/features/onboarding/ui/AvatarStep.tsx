import * as React from "react";
import Picker from "@emoji-mart/react";
import emojiData from "@emoji-mart/data";

import { requestCameraPermission } from "@/shared/api/tauri";
import { cn } from "@/shared/lib/cn";
import { useSmoothCornerClipPath } from "@/shared/lib/useSmoothCornerClipPath";
import { useEmojiBurst } from "@/shared/ui/EmojiBurstProvider";
import { ArcadeSegmentedControl } from "./ArcadeSegmentedControl";
import {
  playOnboardingSound,
  playOnboardingTypingSoundForKey,
} from "./onboardingSounds";
import type { ProfileStepActions, ProfileStepState } from "./types";

type AvatarStepProps = {
  actions: Pick<ProfileStepActions, "updateAvatarUrl">;
  state: Pick<ProfileStepState, "avatar">;
};

type AvatarMode = "image" | "emoji";
type AvatarPreviewKind = "image" | "emoji" | null;
type CameraCaptureStatus = "idle" | "starting" | "active" | "error";

type EmojiMartEmoji = {
  native: string;
};

type DragPoint = {
  clientX: number;
  clientY: number;
};

const AVATAR_COLORS = [
  "#FFFFFF",
  "#F6534F",
  "#FF8652",
  "#FFE75C",
  "#73EF75",
  "#63C6F2",
  "#3399FF",
  "#41EBC1",
  "#B141FF",
  "#FB60C4",
  "#CCCCCC",
  "#000000",
];

const CUSTOM_AVATAR_COLOR_SWATCH = "custom";
const AVATAR_COLOR_SWATCHES = [
  ...AVATAR_COLORS,
  CUSTOM_AVATAR_COLOR_SWATCH,
] as const;
const VISIBLE_AVATAR_COLOR_COUNT = 7;
const AVATAR_COLOR_SWATCH_SIZE = 40;
const AVATAR_COLOR_SWATCH_CELL_SIZE = AVATAR_COLOR_SWATCH_SIZE + 8;
const AVATAR_COLOR_SWATCH_GAP = 12;
const AVATAR_COLOR_SWATCH_STRIDE =
  AVATAR_COLOR_SWATCH_CELL_SIZE + AVATAR_COLOR_SWATCH_GAP;
const AVATAR_COLOR_VIEWPORT_WIDTH =
  VISIBLE_AVATAR_COLOR_COUNT * AVATAR_COLOR_SWATCH_CELL_SIZE +
  (VISIBLE_AVATAR_COLOR_COUNT - 1) * AVATAR_COLOR_SWATCH_GAP;
const DROP_PREVIEW_DRAG_SCALE = 1.12;
const DROP_PREVIEW_OVER_SCALE = 1.35;
const DEFAULT_EMOJI_AVATAR_COLOR = "#FFFFFF";
const DEFAULT_CUSTOM_HUE = 210;
const DEFAULT_CUSTOM_SATURATION = 76;
const DEFAULT_CUSTOM_VALUE = 92;
const CUSTOM_COLOR_GRID_COLUMNS = 15;
const CUSTOM_COLOR_GRID_ROWS = 7;
const CUSTOM_COLOR_GRID_HORIZONTAL_INSET = 24;
const CUSTOM_COLOR_GRID_VERTICAL_INSET = 24;
const CUSTOM_COLOR_GRID_MAGNET_THRESHOLD = 5.5;
const CUSTOM_HUE_SCRUBBER_INSET = 20;
const CUSTOM_COLOR_PANEL_SMOOTHING = 0.6;

const AVATAR_MODE_OPTIONS: Array<{ label: string; value: AvatarMode }> = [
  { label: "Image", value: "image" },
  { label: "Emoji", value: "emoji" },
];

const EMOJI_MART_CATEGORIES = [
  "people",
  "nature",
  "foods",
  "activity",
  "places",
  "objects",
  "symbols",
  "flags",
];

const EMOJI_MART_SHADOW_CSS = `
  #root {
    --padding: 16px;
    --sidebar-width: 0px;
    overflow: hidden;
    width: 100% !important;
  }

  .scroll {
    padding-left: var(--padding);
    padding-right: var(--padding);
    padding-top: 28px;
  }

  .scroll > div {
    width: 100% !important;
  }

  .scroll::-webkit-scrollbar {
    width: 0;
    height: 0;
  }

  .category .sticky {
    display: none;
  }

  .category button .background {
    background-color: rgba(255, 255, 255, 0.1);
  }

  .row {
    justify-content: space-between;
  }

  #nav {
    align-items: center;
    display: flex;
    justify-content: space-between;
    padding: 8px 24px 16px;
  }

  #nav .bar {
    display: none;
  }

  #nav > .relative {
    justify-content: space-between;
    width: 100%;
  }

  #nav button {
    align-items: center;
    border-radius: 999px;
    color: rgba(0, 0, 0, 0.46);
    display: flex;
    flex: 0 0 40px;
    height: 40px;
    justify-content: center;
    transition:
      background-color var(--duration) var(--easing),
      color var(--duration) var(--easing),
      transform var(--duration) var(--easing);
    width: 40px;
  }

  #nav button:hover,
  #nav button[aria-selected] {
    color: rgba(0, 0, 0, 0.62);
  }

  #nav button:hover {
    background-color: rgba(255, 255, 255, 0.1);
  }

  #nav button[aria-selected] {
    background-color: rgba(0, 0, 0, 0.06);
  }

  #nav svg,
  #nav img {
    height: 24px;
    width: 24px;
  }
`;

function ArcadePhotoIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg
      aria-hidden="true"
      fill="none"
      height="32"
      viewBox="0 0 32 32"
      width="32"
      xmlns="http://www.w3.org/2000/svg"
      {...props}
    >
      <path
        d="M26 8.5a4.5 4.5 0 0 1 4.5 4.5v11a4.5 4.5 0 0 1-4.5 4.5H6A4.5 4.5 0 0 1 1.5 24V13A4.5 4.5 0 0 1 6 8.5h20Zm-20 3A1.5 1.5 0 0 0 4.5 13v11A1.5 1.5 0 0 0 6 25.5h20a1.5 1.5 0 0 0 1.5-1.5V13a1.5 1.5 0 0 0-1.5-1.5H6ZM16 14a4.5 4.5 0 1 1 0 9 4.5 4.5 0 0 1 0-9Zm0 3a1.5 1.5 0 1 0 0 3 1.5 1.5 0 0 0 0-3Zm0-14.5c1.627 0 2.878.46 3.735.942.426.24.754.484.983.677a4.853 4.853 0 0 1 .384.362l.01.012.005.006.003.003-2.24 1.996.006.006.01.012-.011-.011a1.801 1.801 0 0 0-.103-.093 3.368 3.368 0 0 0-.517-.354C17.789 5.79 17.04 5.5 16 5.5s-1.789.29-2.265.558c-.24.135-.413.266-.517.354a1.801 1.801 0 0 0-.114.104l.01-.012.006-.006-2.24-1.996.003-.003.005-.006.01-.012a2.308 2.308 0 0 1 .112-.115c.067-.066.158-.15.272-.247.23-.193.557-.437.983-.677C13.122 2.96 14.373 2.5 16 2.5Z"
        fill="currentColor"
      />
    </svg>
  );
}

function ArcadeShareIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg
      aria-hidden="true"
      fill="none"
      height="32"
      viewBox="0 0 32 32"
      width="32"
      xmlns="http://www.w3.org/2000/svg"
      {...props}
    >
      <path
        d="M6.5 24.5h19V19h3v7a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 26v-7h3v5.5Zm9.5-23a1.5 1.5 0 0 1 1.06.44l6 6-2.12 2.12-3.44-3.439V20h-3V6.621l-3.44 3.44-2.12-2.122 6-6 .11-.1A1.5 1.5 0 0 1 16 1.5Z"
        fill="currentColor"
      />
    </svg>
  );
}

function ArcadeHyperlinkIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg
      aria-hidden="true"
      fill="none"
      height="16"
      viewBox="0 0 16 16"
      width="16"
      xmlns="http://www.w3.org/2000/svg"
      {...props}
    >
      <path
        d="m13.576 7.906.142.149a4.01 4.01 0 0 1 0 5.371l-.142.15a4.01 4.01 0 0 1-5.521.142l-.15-.143-1.382-1.382 1.06-1.06 1.383 1.382.19.173a2.51 2.51 0 0 0 3.359-.173l.173-.19c.75-.92.75-2.249 0-3.169l-.173-.19-1.383-1.383 1.06-1.06 1.384 1.383Zm-2.204 2.406-1.06 1.06L4.628 5.69l1.06-1.06 5.684 5.683Zm-8.797-8.03a4.01 4.01 0 0 1 5.37 0l.15.143.02.02L9.55 3.99 8.45 5.01 7.036 3.486l-.19-.173a2.511 2.511 0 0 0-3.169 0l-.19.173a2.51 2.51 0 0 0 0 3.548l1.383 1.383-1.061 1.06-1.383-1.382a4.009 4.009 0 0 1 0-5.67l.15-.143Z"
        fill="currentColor"
      />
    </svg>
  );
}

function ArcadeAddIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg
      aria-hidden="true"
      fill="none"
      height="24"
      viewBox="0 0 24 24"
      width="24"
      xmlns="http://www.w3.org/2000/svg"
      {...props}
    >
      <path d="M13 11h9v2h-9v9h-2v-9H2v-2h9V2h2v9Z" fill="currentColor" />
    </svg>
  );
}

function ArcadeCaretDownIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg
      aria-hidden="true"
      fill="none"
      height="16"
      viewBox="0 0 16 16"
      width="16"
      xmlns="http://www.w3.org/2000/svg"
      {...props}
    >
      <path
        d="m13.06 7.06-4.5 4.5a.75.75 0 0 1-1.06 0L3 7.06 4.06 6l3.97 3.97L12 6l1.06 1.06Z"
        fill="currentColor"
      />
    </svg>
  );
}

function fileToDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result === "string") {
        resolve(reader.result);
      } else {
        reject(new Error("Could not read that image."));
      }
    };
    reader.onerror = () => reject(new Error("Could not read that image."));
    reader.readAsDataURL(file);
  });
}

function emojiAvatarDataUrl(emoji: string, color: string) {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512"><rect width="512" height="512" rx="256" fill="${color}"/><text x="50%" y="54%" dominant-baseline="middle" text-anchor="middle" font-size="172">${emoji}</text></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

function hsvToHex(hue: number, saturation: number, value: number) {
  const normalizedHue = ((hue % 360) + 360) % 360;
  const chroma = (value / 100) * (saturation / 100);
  const huePrime = normalizedHue / 60;
  const secondary = chroma * (1 - Math.abs((huePrime % 2) - 1));
  const match = value / 100 - chroma;
  let red = 0;
  let green = 0;
  let blue = 0;

  if (huePrime >= 0 && huePrime < 1) {
    red = chroma;
    green = secondary;
  } else if (huePrime < 2) {
    red = secondary;
    green = chroma;
  } else if (huePrime < 3) {
    green = chroma;
    blue = secondary;
  } else if (huePrime < 4) {
    green = secondary;
    blue = chroma;
  } else if (huePrime < 5) {
    red = secondary;
    blue = chroma;
  } else {
    red = chroma;
    blue = secondary;
  }

  return [red, green, blue]
    .map((channel) =>
      Math.round((channel + match) * 255)
        .toString(16)
        .padStart(2, "0"),
    )
    .join("")
    .toUpperCase()
    .padStart(6, "0")
    .replace(/^/, "#");
}

function hexToHsv(hexColor: string) {
  const match = hexColor.match(/^#?([0-9a-f]{6})$/i);
  if (!match) {
    return {
      hue: DEFAULT_CUSTOM_HUE,
      saturation: DEFAULT_CUSTOM_SATURATION,
      value: DEFAULT_CUSTOM_VALUE,
    };
  }

  const value = match[1];
  const red = Number.parseInt(value.slice(0, 2), 16) / 255;
  const green = Number.parseInt(value.slice(2, 4), 16) / 255;
  const blue = Number.parseInt(value.slice(4, 6), 16) / 255;
  const max = Math.max(red, green, blue);
  const min = Math.min(red, green, blue);
  const delta = max - min;
  let hue = 0;

  if (delta !== 0) {
    if (max === red) {
      hue = 60 * (((green - blue) / delta) % 6);
    } else if (max === green) {
      hue = 60 * ((blue - red) / delta + 2);
    } else {
      hue = 60 * ((red - green) / delta + 4);
    }
  }

  return {
    hue: Math.round((hue + 360) % 360),
    saturation: max === 0 ? 0 : Math.round((delta / max) * 100),
    value: Math.round(max * 100),
  };
}

function clampPercent(value: number) {
  return Math.max(0, Math.min(100, value));
}

function magnetizeToGrid(value: number, gridCount: number) {
  if (gridCount <= 1) {
    return clampPercent(value);
  }

  const step = 100 / (gridCount - 1);
  const nearest = Math.round(value / step) * step;
  if (Math.abs(nearest - value) <= CUSTOM_COLOR_GRID_MAGNET_THRESHOLD) {
    return nearest;
  }

  return value;
}

function gridInsetPosition(value: number, inset: number) {
  return `calc(${inset}px + (${value} * (100% - ${inset * 2}px) / 100))`;
}

function hueScrubberPosition(value: number) {
  return `calc(${CUSTOM_HUE_SCRUBBER_INSET}px + (${value} * (100% - ${
    CUSTOM_HUE_SCRUBBER_INSET * 2
  }px) / 100))`;
}

function normalizeHue(hue: number) {
  return ((hue % 360) + 360) % 360;
}

function dataTransferHasImage(dataTransfer: DataTransfer | null) {
  if (!dataTransfer) {
    return false;
  }

  const items = Array.from(dataTransfer.items);
  if (items.length > 0) {
    return items.some(
      (item) => item.kind === "file" && item.type.startsWith("image/"),
    );
  }

  return Array.from(dataTransfer.files).some((file) =>
    file.type.startsWith("image/"),
  );
}

function isPointInsidePreviewCircle(
  point: DragPoint,
  target: HTMLElement | null,
) {
  if (!target) {
    return false;
  }

  const rect = target.getBoundingClientRect();
  const centerX = rect.left + rect.width / 2;
  const centerY = rect.top + rect.height / 2;
  const radius = Math.min(target.offsetWidth, target.offsetHeight) / 2;

  return Math.hypot(point.clientX - centerX, point.clientY - centerY) <= radius;
}

function stopMediaStream(stream: MediaStream | null) {
  stream?.getTracks().forEach((track) => {
    track.stop();
  });
}

function resolveCameraErrorMessage(error: unknown) {
  if (error instanceof DOMException) {
    if (error.name === "NotAllowedError") {
      return "Camera access is blocked.";
    }

    if (error.name === "NotFoundError") {
      return "No camera was found.";
    }
  }

  return "Camera is unavailable.";
}

function isCameraPermissionDenied(error: unknown) {
  return error instanceof DOMException && error.name === "NotAllowedError";
}

const USER_CAMERA_CONSTRAINTS = {
  audio: false,
  video: {
    facingMode: "user",
    height: { ideal: 1280 },
    width: { ideal: 1280 },
  },
} satisfies MediaStreamConstraints;

async function requestUserCameraStream() {
  try {
    return await navigator.mediaDevices.getUserMedia(USER_CAMERA_CONSTRAINTS);
  } catch (error) {
    if (!isCameraPermissionDenied(error)) {
      throw error;
    }

    try {
      const nativeGranted = await requestCameraPermission();
      if (!nativeGranted) {
        throw error;
      }
      return await navigator.mediaDevices.getUserMedia(USER_CAMERA_CONSTRAINTS);
    } catch (recoveryError) {
      throw recoveryError instanceof DOMException ? recoveryError : error;
    }
  }
}

function captureVideoFrame(video: HTMLVideoElement) {
  if (video.videoWidth === 0 || video.videoHeight === 0) {
    return null;
  }

  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d");
  if (!context) {
    return null;
  }

  const outputSize = 512;
  const sourceSize = Math.min(video.videoWidth, video.videoHeight);
  const sourceX = (video.videoWidth - sourceSize) / 2;
  const sourceY = (video.videoHeight - sourceSize) / 2;

  canvas.width = outputSize;
  canvas.height = outputSize;
  context.translate(outputSize, 0);
  context.scale(-1, 1);
  context.drawImage(
    video,
    sourceX,
    sourceY,
    sourceSize,
    sourceSize,
    0,
    0,
    outputSize,
    outputSize,
  );

  return canvas.toDataURL("image/jpeg", 0.92);
}

export function AvatarStep({ actions, state }: AvatarStepProps) {
  const { updateAvatarUrl } = actions;
  const avatarUrl = state.avatar.draftUrl.trim();
  const { burstEmoji } = useEmojiBurst();
  const [cameraErrorMessage, setCameraErrorMessage] = React.useState<
    string | null
  >(null);
  const [cameraStatus, setCameraStatus] =
    React.useState<CameraCaptureStatus>("idle");
  const [mode, setMode] = React.useState<AvatarMode>("image");
  const [avatarPreviewKind, setAvatarPreviewKind] =
    React.useState<AvatarPreviewKind>(null);
  const [isCameraOpen, setIsCameraOpen] = React.useState(false);
  const [isDragOverPreview, setIsDragOverPreview] = React.useState(false);
  const [isDragging, setIsDragging] = React.useState(false);
  const [urlDraft, setUrlDraft] = React.useState("");
  const [selectedEmoji, setSelectedEmoji] = React.useState<string | null>(null);
  const [selectedColor, setSelectedColor] = React.useState(
    DEFAULT_EMOJI_AVATAR_COLOR,
  );
  const [customHue, setCustomHue] = React.useState(DEFAULT_CUSTOM_HUE);
  const [customSaturation, setCustomSaturation] = React.useState(
    DEFAULT_CUSTOM_SATURATION,
  );
  const [customValue, setCustomValue] = React.useState(DEFAULT_CUSTOM_VALUE);
  const [isCustomColorPickerOpen, setIsCustomColorPickerOpen] =
    React.useState(false);
  const [colorStartIndex, setColorStartIndex] = React.useState(0);
  const browseInputRef = React.useRef<HTMLInputElement | null>(null);
  const cameraInputRef = React.useRef<HTMLInputElement | null>(null);
  const cameraStreamRef = React.useRef<MediaStream | null>(null);
  const cameraVideoRef = React.useRef<HTMLVideoElement | null>(null);
  const dropPreviewRef = React.useRef<HTMLDivElement | null>(null);
  const emojiPickerContainerRef = React.useRef<HTMLDivElement | null>(null);
  const imageDragClearTimerRef = React.useRef<number | null>(null);
  const imageDragDepthRef = React.useRef(0);
  const hueDragUserSelectRef = React.useRef<string | null>(null);
  const urlInputRef = React.useRef<HTMLInputElement | null>(null);
  const customColorSpectrumClip = useSmoothCornerClipPath<HTMLDivElement>({
    cornerRadius: 24,
    cornerSmoothing: CUSTOM_COLOR_PANEL_SMOOTHING,
  });

  const maxColorStartIndex = Math.max(
    AVATAR_COLOR_SWATCHES.length - VISIBLE_AVATAR_COLOR_COUNT,
    0,
  );
  const customColorDraft = React.useMemo(
    () => hsvToHex(customHue, customSaturation, customValue),
    [customHue, customSaturation, customValue],
  );

  const unlockHueDragSelection = React.useCallback(() => {
    if (hueDragUserSelectRef.current === null) {
      return;
    }

    document.body.style.userSelect = hueDragUserSelectRef.current;
    hueDragUserSelectRef.current = null;
  }, []);

  const lockHueDragSelection = React.useCallback(() => {
    if (hueDragUserSelectRef.current !== null) {
      return;
    }

    hueDragUserSelectRef.current = document.body.style.userSelect;
    document.body.style.userSelect = "none";
  }, []);

  const applyEmojiAvatar = React.useCallback(
    (emoji: string, color = selectedColor) => {
      setAvatarPreviewKind("emoji");
      updateAvatarUrl(emojiAvatarDataUrl(emoji, color));
    },
    [selectedColor, updateAvatarUrl],
  );

  const playAvatarModePressSound = React.useCallback((option: AvatarMode) => {
    playOnboardingSound(option === "image" ? "minorB" : "minorA");
  }, []);

  const openCustomColorPicker = React.useCallback(() => {
    const nextColor = hexToHsv(selectedColor);
    setCustomHue(normalizeHue(nextColor.hue));
    setCustomSaturation(nextColor.saturation);
    setCustomValue(nextColor.value);
    setIsCustomColorPickerOpen(true);
  }, [selectedColor]);

  const updateCustomColorFromPointer = React.useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const rect = event.currentTarget.getBoundingClientRect();
      const width = Math.max(
        rect.width - CUSTOM_COLOR_GRID_HORIZONTAL_INSET * 2,
        1,
      );
      const height = Math.max(
        rect.height - CUSTOM_COLOR_GRID_VERTICAL_INSET * 2,
        1,
      );
      const rawSaturation = clampPercent(
        ((event.clientX - rect.left - CUSTOM_COLOR_GRID_HORIZONTAL_INSET) /
          width) *
          100,
      );
      const rawValue = clampPercent(
        (1 -
          (event.clientY - rect.top - CUSTOM_COLOR_GRID_VERTICAL_INSET) /
            height) *
          100,
      );
      const nextSaturation = magnetizeToGrid(
        rawSaturation,
        CUSTOM_COLOR_GRID_COLUMNS,
      );
      const nextValue = magnetizeToGrid(rawValue, CUSTOM_COLOR_GRID_ROWS);

      setCustomSaturation(Math.round(nextSaturation));
      setCustomValue(Math.round(nextValue));
    },
    [],
  );

  const updateCustomHueFromPointer = React.useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const rect = event.currentTarget.getBoundingClientRect();
      const trackWidth = Math.max(
        rect.width - CUSTOM_HUE_SCRUBBER_INSET * 2,
        1,
      );
      const nextPercent = clampPercent(
        ((event.clientX - rect.left - CUSTOM_HUE_SCRUBBER_INSET) / trackWidth) *
          100,
      );
      setCustomHue(Math.round((nextPercent / 100) * 360));
    },
    [],
  );

  const adjustCustomHue = React.useCallback((delta: number) => {
    setCustomHue((current) => normalizeHue(current + delta));
  }, []);

  const commitCustomColor = React.useCallback(() => {
    setSelectedColor(customColorDraft);
    if (selectedEmoji) {
      applyEmojiAvatar(selectedEmoji, customColorDraft);
    }
    setIsCustomColorPickerOpen(false);
  }, [applyEmojiAvatar, customColorDraft, selectedEmoji]);

  const handleFiles = React.useCallback(
    async (files: FileList | null) => {
      const file = files?.[0];
      if (!file) {
        return;
      }

      if (!file.type.startsWith("image/")) {
        return;
      }

      const dataUrl = await fileToDataUrl(file);
      setAvatarPreviewKind("image");
      updateAvatarUrl(dataUrl);
      setMode("image");
    },
    [updateAvatarUrl],
  );

  const updateImageDragState = React.useCallback((point: DragPoint) => {
    setIsDragging(true);
    setIsDragOverPreview(
      isPointInsidePreviewCircle(point, dropPreviewRef.current),
    );
  }, []);

  const clearImageDragState = React.useCallback(() => {
    imageDragDepthRef.current = 0;
    setIsDragging(false);
    setIsDragOverPreview(false);
  }, []);

  const scheduleImageDragStateClear = React.useCallback(() => {
    if (imageDragClearTimerRef.current !== null) {
      window.clearTimeout(imageDragClearTimerRef.current);
    }

    imageDragClearTimerRef.current = window.setTimeout(() => {
      imageDragClearTimerRef.current = null;
      clearImageDragState();
    }, 180);
  }, [clearImageDragState]);

  const cancelImageDragStateClear = React.useCallback(() => {
    if (imageDragClearTimerRef.current === null) {
      return;
    }

    window.clearTimeout(imageDragClearTimerRef.current);
    imageDragClearTimerRef.current = null;
  }, []);

  const stopCameraStream = React.useCallback(() => {
    stopMediaStream(cameraStreamRef.current);
    cameraStreamRef.current = null;
    if (cameraVideoRef.current) {
      cameraVideoRef.current.srcObject = null;
    }
  }, []);

  const closeCamera = React.useCallback(() => {
    stopCameraStream();
    setCameraStatus("idle");
    setCameraErrorMessage(null);
    setIsCameraOpen(false);
  }, [stopCameraStream]);

  const openCamera = React.useCallback(async () => {
    if (!navigator.mediaDevices?.getUserMedia) {
      cameraInputRef.current?.click();
      return;
    }

    setMode("image");
    setIsCameraOpen(true);
    setCameraStatus("starting");
    setCameraErrorMessage(null);
  }, []);

  const captureCameraPhoto = React.useCallback(() => {
    const video = cameraVideoRef.current;
    if (!video || cameraStatus !== "active") {
      return;
    }

    const dataUrl = captureVideoFrame(video);
    if (!dataUrl) {
      setCameraStatus("error");
      setCameraErrorMessage("Could not capture that photo.");
      return;
    }

    updateAvatarUrl(dataUrl);
    setAvatarPreviewKind("image");
    setMode("image");
    closeCamera();
  }, [cameraStatus, closeCamera, updateAvatarUrl]);

  React.useEffect(() => unlockHueDragSelection, [unlockHueDragSelection]);

  React.useEffect(() => {
    let animationFrame = 0;

    const installEmojiMartStyles = () => {
      const host =
        emojiPickerContainerRef.current?.querySelector("em-emoji-picker");
      const shadowRoot = host?.shadowRoot;

      if (!shadowRoot) {
        animationFrame = window.requestAnimationFrame(installEmojiMartStyles);
        return;
      }

      if (!shadowRoot.querySelector("#sprout-emoji-mart-style")) {
        const style = document.createElement("style");
        style.id = "sprout-emoji-mart-style";
        style.textContent = EMOJI_MART_SHADOW_CSS;
        shadowRoot.appendChild(style);
      }
    };

    animationFrame = window.requestAnimationFrame(installEmojiMartStyles);

    return () => {
      window.cancelAnimationFrame(animationFrame);
    };
  }, []);

  React.useEffect(() => {
    if (!isCameraOpen || cameraStatus !== "starting") {
      return;
    }

    let cancelled = false;

    async function startCamera() {
      setCameraErrorMessage(null);

      try {
        const stream = await requestUserCameraStream();

        if (cancelled) {
          stopMediaStream(stream);
          return;
        }

        cameraStreamRef.current = stream;

        const video = cameraVideoRef.current;
        if (video) {
          video.srcObject = stream;
          await video.play().catch(() => {});
        }

        if (!cancelled) {
          setCameraStatus("active");
        }
      } catch (error) {
        if (!cancelled) {
          setCameraStatus("error");
          setCameraErrorMessage(resolveCameraErrorMessage(error));
        }
      }
    }

    void startCamera();

    return () => {
      cancelled = true;
      stopCameraStream();
    };
  }, [cameraStatus, isCameraOpen, stopCameraStream]);

  React.useEffect(() => {
    if (!isCameraOpen) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        closeCamera();
      }
    };

    window.addEventListener("keydown", handleKeyDown);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [closeCamera, isCameraOpen]);

  React.useEffect(() => {
    const handleWindowDrag = (event: DragEvent) => {
      if (!dataTransferHasImage(event.dataTransfer)) {
        return;
      }

      event.preventDefault();
      if (event.dataTransfer) {
        event.dataTransfer.dropEffect = "copy";
      }
      cancelImageDragStateClear();
      updateImageDragState(event);
    };

    const handleWindowDragLeave = (event: DragEvent) => {
      const hasLeftWindow =
        event.clientX <= 0 ||
        event.clientY <= 0 ||
        event.clientX >= window.innerWidth ||
        event.clientY >= window.innerHeight;

      if (hasLeftWindow) {
        clearImageDragState();
      } else {
        scheduleImageDragStateClear();
      }
    };

    const handleWindowDrop = (event: DragEvent) => {
      if (!dataTransferHasImage(event.dataTransfer)) {
        return;
      }

      event.preventDefault();
      clearImageDragState();
      void handleFiles(event.dataTransfer?.files ?? null);
    };

    window.addEventListener("dragenter", handleWindowDrag);
    window.addEventListener("dragover", handleWindowDrag);
    window.addEventListener("dragleave", handleWindowDragLeave);
    window.addEventListener("drop", handleWindowDrop);
    window.addEventListener("dragend", clearImageDragState);
    window.addEventListener("blur", clearImageDragState);

    return () => {
      window.removeEventListener("dragenter", handleWindowDrag);
      window.removeEventListener("dragover", handleWindowDrag);
      window.removeEventListener("dragleave", handleWindowDragLeave);
      window.removeEventListener("drop", handleWindowDrop);
      window.removeEventListener("dragend", clearImageDragState);
      window.removeEventListener("blur", clearImageDragState);
      cancelImageDragStateClear();
    };
  }, [
    cancelImageDragStateClear,
    clearImageDragState,
    handleFiles,
    scheduleImageDragStateClear,
    updateImageDragState,
  ]);

  const applyUrl = React.useCallback(() => {
    const nextUrl = urlDraft.trim();
    if (nextUrl.length === 0) {
      return;
    }
    setAvatarPreviewKind("image");
    updateAvatarUrl(nextUrl);
    setMode("image");
  }, [updateAvatarUrl, urlDraft]);

  const isEmojiAvatarPreview =
    avatarPreviewKind === "emoji" && avatarUrl.length > 0;
  const emojiPreviewColor =
    isCustomColorPickerOpen && selectedEmoji ? customColorDraft : selectedColor;
  const previewScale = isCameraOpen
    ? 1.5
    : isDragOverPreview
      ? DROP_PREVIEW_OVER_SCALE
      : isDragging
        ? DROP_PREVIEW_DRAG_SCALE
        : 1;

  return (
    <fieldset
      className="relative m-0 flex w-full max-w-[576px] flex-col items-center border-0 p-0"
      data-testid="onboarding-avatar-step"
      onDragEnter={(event) => {
        if (!dataTransferHasImage(event.dataTransfer)) {
          return;
        }
        event.preventDefault();
        event.stopPropagation();
        cancelImageDragStateClear();
        imageDragDepthRef.current += 1;
        updateImageDragState(event);
      }}
      onDragLeave={(event) => {
        if (!dataTransferHasImage(event.dataTransfer)) {
          return;
        }
        event.preventDefault();
        event.stopPropagation();
        imageDragDepthRef.current = Math.max(imageDragDepthRef.current - 1, 0);
        if (imageDragDepthRef.current === 0) {
          scheduleImageDragStateClear();
        }
      }}
      onDragOver={(event) => {
        if (!dataTransferHasImage(event.dataTransfer)) {
          return;
        }
        event.preventDefault();
        event.stopPropagation();
        event.dataTransfer.dropEffect = "copy";
        cancelImageDragStateClear();
        updateImageDragState(event);
      }}
      onDrop={(event) => {
        if (!dataTransferHasImage(event.dataTransfer)) {
          return;
        }
        event.preventDefault();
        event.stopPropagation();
        clearImageDragState();
        void handleFiles(event.dataTransfer.files);
      }}
    >
      <legend className="sr-only">Avatar image picker</legend>
      <div
        aria-hidden="true"
        className={cn(
          "pointer-events-none absolute -inset-6 z-20 rounded-[32px] bg-[#F2F2F2]/80 opacity-0 backdrop-blur-[2px] transition-[opacity,backdrop-filter] duration-300 ease-out",
          isDragging && "opacity-100",
        )}
        data-testid="onboarding-avatar-drop-mask"
      />
      <div className="relative z-30 flex h-[192px] items-center justify-center">
        <div
          className={cn(
            "flex h-[192px] w-[192px] transform-gpu items-center justify-center overflow-hidden rounded-full border-2 border-dashed border-[#9B9B9B] text-[#8E8E8E] transition-[background-color,border-color,box-shadow,transform] duration-300 ease-out will-change-transform",
            isDragging &&
              "border-black bg-white shadow-[0_24px_80px_rgba(0,0,0,0.18)]",
            avatarUrl && "border-transparent bg-transparent",
            isEmojiAvatarPreview &&
              "border-transparent bg-[var(--arcade-semantic-border-subtle)]",
            isCameraOpen &&
              "border-0 bg-black shadow-[0_24px_80px_rgba(0,0,0,0.18)]",
          )}
          ref={dropPreviewRef}
          style={{ transform: `scale(${previewScale})` }}
        >
          {isCameraOpen ? (
            <>
              <video
                autoPlay
                className={cn(
                  "h-full w-full -scale-x-100 object-cover transition-opacity duration-300 ease-out",
                  cameraStatus === "active" ? "opacity-100" : "opacity-0",
                )}
                muted
                playsInline
                ref={cameraVideoRef}
              />
              {cameraStatus !== "active" ? (
                <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-black text-white">
                  {cameraStatus === "starting" ? (
                    <div className="h-8 w-8 rounded-full border-2 border-white/25 border-t-white motion-safe:animate-spin" />
                  ) : (
                    <>
                      <p className="arcade-type-label-small max-w-[120px] text-center text-white/75">
                        {cameraErrorMessage ?? "Camera is unavailable."}
                      </p>
                      <button
                        className="arcade-type-label-small rounded-full bg-white px-4 py-2 text-black transition-colors hover:bg-white/90"
                        onClick={() => {
                          closeCamera();
                          cameraInputRef.current?.click();
                        }}
                        type="button"
                      >
                        Browse
                      </button>
                    </>
                  )}
                </div>
              ) : null}
            </>
          ) : (
            <button
              aria-label="Choose display image"
              className="flex h-full w-full items-center justify-center"
              onClick={() => browseInputRef.current?.click()}
              type="button"
            >
              {isEmojiAvatarPreview ? (
                <div
                  className="flex h-[calc(100%+8px)] w-[calc(100%+8px)] shrink-0 items-center justify-center rounded-full transition-colors duration-300 ease-out"
                  style={{ backgroundColor: emojiPreviewColor }}
                >
                  <span
                    className="text-[64px] leading-none [animation:sprout-avatar-emoji-in_300ms_ease-out]"
                    key={selectedEmoji}
                  >
                    {selectedEmoji}
                  </span>
                </div>
              ) : avatarUrl ? (
                <img
                  alt="Selected avatar"
                  className="h-full w-full rounded-full object-cover [animation:sprout-avatar-preview-in_300ms_ease-out]"
                  key={avatarUrl}
                  src={avatarUrl}
                />
              ) : (
                <ArcadeAddIcon className="h-16 w-16" />
              )}
            </button>
          )}
        </div>

        {isCameraOpen ? (
          <div className="absolute left-1/2 top-[252px] z-40 flex w-[288px] -translate-x-1/2 items-center gap-3">
            <button
              className="arcade-type-label-small h-14 rounded-full bg-white px-6 text-black shadow-[0_12px_40px_rgba(0,0,0,0.08)] transition-colors hover:bg-white/90"
              onClick={closeCamera}
              type="button"
            >
              Cancel
            </button>
            <button
              className="arcade-type-body-medium h-14 flex-1 rounded-full bg-black px-6 text-white shadow-[0_12px_40px_rgba(0,0,0,0.14)] transition-colors hover:bg-black/85 disabled:bg-black/35 disabled:text-white/50"
              disabled={cameraStatus !== "active"}
              onClick={captureCameraPhoto}
              type="button"
            >
              Capture
            </button>
          </div>
        ) : null}
      </div>

      <div
        className={cn(
          "mt-12 grid w-full gap-4 transition-opacity duration-200 ease-out",
          isCameraOpen && "pointer-events-none opacity-0",
        )}
      >
        <ArcadeSegmentedControl
          aria-label="Avatar type"
          onChange={(option) => {
            setMode(option);
            setIsCustomColorPickerOpen(false);
          }}
          onOptionPress={playAvatarModePressSound}
          options={AVATAR_MODE_OPTIONS}
          value={mode}
        />

        <div className="relative min-h-[392px] overflow-visible">
          <div
            className="absolute inset-0 grid content-start gap-3 transition-[opacity,transform] duration-300 ease-[cubic-bezier(0.22,1,0.36,1)]"
            style={{
              opacity: mode === "image" ? 1 : 0,
              transform: mode === "image" ? "scale(1)" : "scale(0.95)",
              pointerEvents: mode === "image" ? "auto" : "none",
            }}
          >
            <div className="grid grid-cols-2 gap-3">
              <button
                className="flex h-[120px] flex-col items-center justify-center gap-3 rounded-[12px] bg-[var(--arcade-semantic-background-standard)] text-black transition-colors hover:bg-[var(--arcade-semantic-background-standard-hover)]"
                onClick={openCamera}
                type="button"
              >
                <ArcadePhotoIcon className="h-8 w-8 text-[#8E8E8E]" />
                <span className="arcade-type-label-small">Take photo</span>
              </button>
              <button
                className="flex h-[120px] flex-col items-center justify-center gap-3 rounded-[12px] bg-[var(--arcade-semantic-background-standard)] text-black transition-colors hover:bg-[var(--arcade-semantic-background-standard-hover)]"
                onClick={() => browseInputRef.current?.click()}
                type="button"
              >
                <ArcadeShareIcon className="h-8 w-8 text-[#8E8E8E]" />
                <span className="arcade-type-label-small">
                  Drop or{" "}
                  <span className="underline underline-offset-2">Browse</span>
                </span>
              </button>
            </div>

            <div className="flex h-16 items-center gap-3 rounded-[12px] bg-[var(--arcade-semantic-background-standard)] px-5 transition-colors focus-within:bg-white">
              <ArcadeHyperlinkIcon className="h-4 w-4 text-[#8E8E8E]" />
              <input
                className="arcade-type-label-small min-w-0 flex-1 bg-transparent text-black outline-none placeholder:text-black"
                onBlur={applyUrl}
                onChange={(event) => setUrlDraft(event.target.value)}
                onKeyDown={(event) => {
                  playOnboardingTypingSoundForKey(event);
                  if (event.key === "Enter") {
                    applyUrl();
                  }
                }}
                placeholder="Paste a URL (Slack profile, etc.)"
                ref={urlInputRef}
                type="url"
                value={urlDraft}
              />
            </div>
          </div>

          <div
            className="absolute inset-0 grid content-start gap-3 transition-[opacity,transform] duration-300 ease-[cubic-bezier(0.22,1,0.36,1)]"
            style={{
              opacity: mode === "emoji" ? 1 : 0,
              transform: mode === "emoji" ? "scale(1)" : "scale(0.95)",
              pointerEvents: mode === "emoji" ? "auto" : "none",
            }}
          >
            <div
              className={cn(
                "sprout-emoji-mart relative z-0 h-[316px] overflow-hidden rounded-[32px] bg-[var(--arcade-semantic-background-standard)] transition-opacity duration-200 ease-out",
                isCustomColorPickerOpen && "pointer-events-none opacity-0",
              )}
              ref={emojiPickerContainerRef}
            >
              <Picker
                categories={EMOJI_MART_CATEGORIES}
                data={emojiData}
                dynamicWidth={false}
                emojiButtonRadius="999px"
                emojiButtonSize={64}
                emojiSize={48}
                icons="outline"
                navPosition="bottom"
                onEmojiSelect={(emoji: EmojiMartEmoji, event?: MouseEvent) => {
                  playOnboardingSound("changeTheme");
                  burstEmoji(emoji.native, event);
                  setSelectedEmoji(emoji.native);
                  applyEmojiAvatar(emoji.native, selectedColor);
                }}
                perLine={6}
                previewPosition="none"
                searchPosition="none"
                set="native"
                skinTonePosition="none"
                theme="light"
              />
            </div>

            <div
              className={cn(
                "relative flex h-16 items-center justify-center rounded-[12px] bg-[var(--arcade-semantic-background-standard)] px-4 transition-opacity duration-200 ease-out",
                isCustomColorPickerOpen && "pointer-events-none opacity-0",
              )}
            >
              <button
                aria-label="Show previous avatar colors"
                className="absolute left-4 flex h-12 w-12 items-center justify-center rounded-full text-[#8E8E8E] transition-colors hover:bg-white/50 disabled:text-[#8E8E8E]/35 disabled:hover:bg-transparent"
                disabled={colorStartIndex === 0}
                onClick={() =>
                  setColorStartIndex((current) =>
                    Math.max(current - VISIBLE_AVATAR_COLOR_COUNT, 0),
                  )
                }
                type="button"
              >
                <ArcadeCaretDownIcon className="h-7 w-7 rotate-90" />
              </button>
              <div
                className="mx-auto overflow-hidden"
                style={{ width: AVATAR_COLOR_VIEWPORT_WIDTH }}
              >
                <div
                  className="flex items-center transition-transform duration-300 ease-[cubic-bezier(0.22,1,0.36,1)]"
                  style={{
                    gap: AVATAR_COLOR_SWATCH_GAP,
                    transform: `translateX(-${
                      colorStartIndex * AVATAR_COLOR_SWATCH_STRIDE
                    }px)`,
                  }}
                >
                  {AVATAR_COLOR_SWATCHES.map((swatch) => {
                    const isCustomSwatch =
                      swatch === CUSTOM_AVATAR_COLOR_SWATCH;
                    const isSelected = isCustomSwatch
                      ? !AVATAR_COLORS.some(
                          (color) =>
                            color.toUpperCase() === selectedColor.toUpperCase(),
                        )
                      : swatch.toUpperCase() === selectedColor.toUpperCase();

                    return (
                      <div
                        className="flex h-12 w-12 shrink-0 items-center justify-center"
                        key={swatch}
                      >
                        <button
                          aria-label={
                            isCustomSwatch
                              ? "Choose custom avatar color"
                              : `Use ${swatch} background`
                          }
                          aria-pressed={isSelected}
                          className={cn(
                            "relative h-10 w-10 rounded-full border border-[var(--arcade-semantic-border-subtle)] transition-transform duration-200 ease-[cubic-bezier(0.22,1,0.36,1)] hover:scale-[1.15] focus-visible:scale-[1.15] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-black/20",
                          )}
                          onPointerDown={() => {
                            if (isCustomSwatch || !isSelected) {
                              playOnboardingSound("toggleMinor");
                            }
                          }}
                          onClick={() => {
                            if (isCustomSwatch) {
                              openCustomColorPicker();
                              return;
                            }

                            setSelectedColor(swatch);
                            if (selectedEmoji) {
                              applyEmojiAvatar(selectedEmoji, swatch);
                            }
                          }}
                          style={{
                            background: isCustomSwatch
                              ? isSelected
                                ? selectedColor
                                : "conic-gradient(from 0deg, #ff4d4d, #ffe75c, #73ef75, #63c6f2, #b141ff, #ff4d4d)"
                              : swatch,
                          }}
                          type="button"
                        >
                          {isSelected ? (
                            <span
                              className="absolute inset-1 rounded-full border-[3px]"
                              style={{
                                borderColor:
                                  !isCustomSwatch &&
                                  swatch.toUpperCase() === "#FFFFFF"
                                    ? "#000000"
                                    : "#FFFFFF",
                              }}
                            />
                          ) : null}
                        </button>
                      </div>
                    );
                  })}
                </div>
              </div>
              <button
                aria-label="Show more avatar colors"
                className="absolute right-4 flex h-12 w-12 items-center justify-center rounded-full text-black transition-colors hover:bg-white/50 disabled:text-black/25 disabled:hover:bg-transparent"
                disabled={colorStartIndex >= maxColorStartIndex}
                onClick={() =>
                  setColorStartIndex((current) =>
                    Math.min(
                      current + VISIBLE_AVATAR_COLOR_COUNT,
                      maxColorStartIndex,
                    ),
                  )
                }
                type="button"
              >
                <ArcadeCaretDownIcon className="h-7 w-7 -rotate-90" />
              </button>
            </div>

            <div
              className={cn(
                "absolute inset-x-0 top-0 bottom-0 z-[9999] flex origin-bottom flex-col rounded-[32px] bg-[var(--arcade-semantic-background-standard)] p-4 transition-[opacity,transform] duration-300 ease-[cubic-bezier(0.22,1,0.36,1)]",
                isCustomColorPickerOpen
                  ? "pointer-events-auto translate-y-0 scale-y-100 opacity-100"
                  : "pointer-events-none translate-y-8 scale-y-[0.94] opacity-0",
              )}
            >
              <div
                ref={customColorSpectrumClip.ref}
                className="relative min-h-0 w-full flex-1 cursor-pointer overflow-hidden rounded-[24px] shadow-[inset_0_-18px_34px_rgba(0,0,0,0.1)]"
                data-testid="onboarding-avatar-custom-color-spectrum"
                onPointerDown={(event) => {
                  event.currentTarget.setPointerCapture(event.pointerId);
                  updateCustomColorFromPointer(event);
                }}
                onPointerMove={(event) => {
                  if (event.buttons === 1) {
                    updateCustomColorFromPointer(event);
                  }
                }}
                style={{
                  ...customColorSpectrumClip.style,
                  backgroundColor: `hsl(${customHue}, 100%, 50%)`,
                  backgroundImage:
                    "linear-gradient(to bottom, transparent 0%, #000000 100%), linear-gradient(to right, #ffffff 0%, rgba(255,255,255,0) 100%)",
                }}
              >
                <div
                  aria-hidden="true"
                  className="pointer-events-none absolute"
                  style={{
                    inset: `${CUSTOM_COLOR_GRID_VERTICAL_INSET}px ${CUSTOM_COLOR_GRID_HORIZONTAL_INSET}px`,
                  }}
                >
                  {Array.from({
                    length: CUSTOM_COLOR_GRID_COLUMNS * CUSTOM_COLOR_GRID_ROWS,
                  }).map((_, index) => {
                    const column = index % CUSTOM_COLOR_GRID_COLUMNS;
                    const row = Math.floor(index / CUSTOM_COLOR_GRID_COLUMNS);
                    const gridSaturation = Math.round(
                      (column / (CUSTOM_COLOR_GRID_COLUMNS - 1)) * 100,
                    );
                    const gridValue = Math.round(
                      100 - (row / (CUSTOM_COLOR_GRID_ROWS - 1)) * 100,
                    );
                    const isSelectedGridDot =
                      gridSaturation === customSaturation &&
                      gridValue === customValue;

                    return (
                      <span
                        className={cn(
                          "absolute h-1 w-1 -translate-x-1/2 -translate-y-1/2 rounded-full bg-white/60 shadow-[0_0_4px_rgba(255,255,255,0.24)]",
                          isSelectedGridDot &&
                            "h-3 w-3 border-2 border-white shadow-[0_2px_10px_rgba(0,0,0,0.24)]",
                        )}
                        key={`${column}-${row}`}
                        style={{
                          backgroundColor: isSelectedGridDot
                            ? customColorDraft
                            : undefined,
                          left: `${
                            (column / (CUSTOM_COLOR_GRID_COLUMNS - 1)) * 100
                          }%`,
                          top: `${(row / (CUSTOM_COLOR_GRID_ROWS - 1)) * 100}%`,
                        }}
                      />
                    );
                  })}
                </div>
                <div
                  className="pointer-events-none absolute h-8 w-8 -translate-x-1/2 -translate-y-1/2 rounded-full border-[3px] border-white shadow-[0_5px_16px_rgba(0,0,0,0.24),inset_0_0_0_1px_rgba(0,0,0,0.06)]"
                  style={{
                    backgroundColor: customColorDraft,
                    left: gridInsetPosition(
                      customSaturation,
                      CUSTOM_COLOR_GRID_HORIZONTAL_INSET,
                    ),
                    top: gridInsetPosition(
                      100 - customValue,
                      CUSTOM_COLOR_GRID_VERTICAL_INSET,
                    ),
                  }}
                />
              </div>

              <div
                aria-label="Choose custom avatar color hue"
                aria-valuemax={360}
                aria-valuemin={0}
                aria-valuenow={customHue}
                className="sprout-avatar-hue-scrubber relative mt-3 h-10 w-full cursor-pointer select-none rounded-full touch-none"
                data-testid="onboarding-avatar-custom-color-hue"
                onKeyDown={(event) => {
                  if (event.key === "ArrowLeft" || event.key === "ArrowDown") {
                    event.preventDefault();
                    adjustCustomHue(-6);
                  } else if (
                    event.key === "ArrowRight" ||
                    event.key === "ArrowUp"
                  ) {
                    event.preventDefault();
                    adjustCustomHue(6);
                  } else if (event.key === "Home") {
                    event.preventDefault();
                    setCustomHue(0);
                  } else if (event.key === "End") {
                    event.preventDefault();
                    setCustomHue(360);
                  }
                }}
                onPointerDown={(event) => {
                  event.preventDefault();
                  lockHueDragSelection();
                  event.currentTarget.setPointerCapture(event.pointerId);
                  updateCustomHueFromPointer(event);
                }}
                onPointerMove={(event) => {
                  if (event.buttons === 1) {
                    event.preventDefault();
                    updateCustomHueFromPointer(event);
                  }
                }}
                onPointerCancel={unlockHueDragSelection}
                onPointerUp={unlockHueDragSelection}
                onLostPointerCapture={unlockHueDragSelection}
                role="slider"
                tabIndex={0}
              >
                <div
                  aria-hidden="true"
                  className="absolute top-1 h-8 w-8 -translate-x-1/2 rounded-full"
                  data-testid="onboarding-avatar-custom-color-hue-thumb"
                  style={{
                    left: hueScrubberPosition((customHue / 360) * 100),
                  }}
                >
                  <div className="h-full w-full rounded-full bg-white shadow-[0_5px_18px_rgba(0,0,0,0.24),inset_0_0_0_1px_rgba(0,0,0,0.06)]" />
                </div>
              </div>

              <button
                className="arcade-type-body-medium mt-3 h-12 w-full rounded-full bg-white px-6 text-black transition-colors hover:bg-white/90"
                onClick={commitCustomColor}
                onPointerDown={() => playOnboardingSound("toggleMinor")}
                type="button"
              >
                Done
              </button>
            </div>
          </div>
        </div>
      </div>

      <input
        accept="image/*"
        capture="user"
        className="hidden"
        onChange={(event) => {
          void handleFiles(event.target.files);
          event.target.value = "";
        }}
        ref={cameraInputRef}
        type="file"
      />
      <input
        accept="image/*"
        className="hidden"
        onChange={(event) => {
          void handleFiles(event.target.files);
          event.target.value = "";
        }}
        ref={browseInputRef}
        type="file"
      />
    </fieldset>
  );
}
