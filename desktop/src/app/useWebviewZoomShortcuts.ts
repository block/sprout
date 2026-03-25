import * as React from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";

const DEFAULT_ZOOM_FACTOR = 1;
const MAX_ZOOM_FACTOR = 10;
const ZOOM_STEP = 0.2;

function isZoomInShortcut(event: KeyboardEvent) {
  return (
    (event.metaKey || event.ctrlKey) &&
    !event.altKey &&
    (event.key === "+" ||
      event.key === "=" ||
      event.code === "Equal" ||
      event.code === "NumpadAdd")
  );
}

export function useWebviewZoomShortcuts() {
  const zoomFactorRef = React.useRef(DEFAULT_ZOOM_FACTOR);

  React.useEffect(() => {
    const webview = getCurrentWebview();

    function handleKeyDown(event: KeyboardEvent) {
      if (!isZoomInShortcut(event)) {
        return;
      }

      event.preventDefault();

      const previousZoomFactor = zoomFactorRef.current;
      const nextZoomFactor = Math.min(
        previousZoomFactor + ZOOM_STEP,
        MAX_ZOOM_FACTOR,
      );

      if (nextZoomFactor === previousZoomFactor) {
        return;
      }

      zoomFactorRef.current = nextZoomFactor;

      void webview.setZoom(nextZoomFactor).catch((error) => {
        zoomFactorRef.current = previousZoomFactor;
        console.error("Failed to increase webview zoom", error);
      });
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, []);
}
