import * as React from "react";

const MOBILE_BREAKPOINT = 768;
const THREAD_PANEL_OVERLAY_BREAKPOINT = 1024;

export function useIsMobile() {
  const [isMobile, setIsMobile] = React.useState<boolean | undefined>(
    undefined,
  );

  React.useEffect(() => {
    const mql = window.matchMedia(`(max-width: ${MOBILE_BREAKPOINT - 1}px)`);
    const onChange = () => {
      setIsMobile(window.innerWidth < MOBILE_BREAKPOINT);
    };
    mql.addEventListener("change", onChange);
    setIsMobile(window.innerWidth < MOBILE_BREAKPOINT);
    return () => mql.removeEventListener("change", onChange);
  }, []);

  return !!isMobile;
}

export function useIsThreadPanelOverlay() {
  const [isOverlay, setIsOverlay] = React.useState<boolean>(() =>
    typeof window !== "undefined"
      ? window.innerWidth < THREAD_PANEL_OVERLAY_BREAKPOINT
      : false,
  );

  React.useEffect(() => {
    const mql = window.matchMedia(
      `(max-width: ${THREAD_PANEL_OVERLAY_BREAKPOINT - 1}px)`,
    );
    const onChange = () => {
      setIsOverlay(window.innerWidth < THREAD_PANEL_OVERLAY_BREAKPOINT);
    };
    mql.addEventListener("change", onChange);
    setIsOverlay(window.innerWidth < THREAD_PANEL_OVERLAY_BREAKPOINT);
    return () => mql.removeEventListener("change", onChange);
  }, []);

  return isOverlay;
}
