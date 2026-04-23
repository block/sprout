import * as React from "react";
import { AlertCircle, Play } from "lucide-react";

import { cn } from "@/shared/lib/cn";

import { Spinner } from "./spinner";

type VideoPlayerState = "idle" | "loading" | "playing" | "error";

type VideoPlayerProps = {
  src: string;
  poster?: string;
};

function OverlayIcon({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-14 w-14 items-center justify-center rounded-full bg-black/50 backdrop-blur-sm">
      {children}
    </div>
  );
}

export function VideoPlayer({ src, poster }: VideoPlayerProps) {
  const [state, setState] = React.useState<VideoPlayerState>("idle");
  const videoRef = React.useRef<HTMLVideoElement>(null);

  const handlePlay = React.useCallback(() => {
    setState("loading");
    const video = videoRef.current;
    if (video) {
      video.load();
      video
        .play()
        .then(() => {
          setState("playing");
        })
        .catch((error) => {
          if (
            error instanceof DOMException &&
            error.name === "NotAllowedError"
          ) {
            setState("playing");
          } else {
            setState("error");
          }
        });
    }
  }, []);

  const handleError = React.useCallback(() => {
    if (state === "loading") {
      setState("error");
    }
  }, [state]);

  return (
    <div className="mt-3 flex max-w-sm items-center justify-center overflow-hidden rounded-2xl border border-border/70 bg-muted/40">
      {state !== "playing" && (
        <div className="relative flex items-center justify-center">
          {poster ? (
            <img
              src={poster}
              alt=""
              className="max-h-64 max-w-full object-contain"
            />
          ) : (
            <div className="flex h-48 w-72 items-center justify-center bg-muted/60" />
          )}
          {state === "idle" && (
            <button
              type="button"
              aria-label="Play video"
              className="absolute inset-0 flex cursor-pointer items-center justify-center transition-opacity hover:opacity-80"
              onClick={handlePlay}
            >
              <OverlayIcon>
                <Play className="h-7 w-7 fill-white text-white" />
              </OverlayIcon>
            </button>
          )}
          {state === "loading" && (
            <div className="absolute inset-0 flex items-center justify-center">
              <OverlayIcon>
                <Spinner className="text-white" size={28} />
              </OverlayIcon>
            </div>
          )}
          {state === "error" && (
            <button
              type="button"
              aria-label="Retry loading video"
              className="absolute inset-0 flex cursor-pointer flex-col items-center justify-center gap-2 transition-opacity hover:opacity-80"
              onClick={handlePlay}
            >
              <OverlayIcon>
                <AlertCircle className="h-7 w-7 text-white" />
              </OverlayIcon>
              <span className="rounded-md bg-black/50 px-2 py-1 text-xs text-white backdrop-blur-sm">
                Failed to load — tap to retry
              </span>
            </button>
          )}
        </div>
      )}
      {/* biome-ignore lint/a11y/useMediaCaption: user-uploaded video, no captions available */}
      <video
        ref={videoRef}
        preload="metadata"
        controls={state === "playing"}
        className={cn(
          "max-h-64 max-w-full object-contain",
          state !== "playing" && "hidden",
        )}
        src={src}
        onError={handleError}
      />
    </div>
  );
}
