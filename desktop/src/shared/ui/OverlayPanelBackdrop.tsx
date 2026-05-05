/**
 * Shared constants and backdrop for thread/agent overlay panels
 * that slide in from the right at narrow viewport widths.
 */

/** Base classes for every side panel `<aside>`. */
export const PANEL_BASE_CLASS =
  "relative flex h-full shrink-0 flex-col border-l border-border/80 bg-background";

/** Extra classes applied when the panel is rendered as a floating overlay. */
export const PANEL_OVERLAY_CLASS =
  "fixed inset-y-0 right-0 z-40 shadow-xl max-w-[calc(100vw-2rem)]";

type OverlayPanelBackdropProps = {
  onClose: () => void;
};

export function OverlayPanelBackdrop({ onClose }: OverlayPanelBackdropProps) {
  return (
    <div
      className="fixed inset-0 z-30 bg-black/20"
      onClick={onClose}
      aria-hidden="true"
    />
  );
}
