import * as React from "react";

import { getSmoothCornerPath } from "./smoothCorners";

type UseSmoothCornerClipPathOptions = {
  cornerRadius: number;
  cornerSmoothing?: number;
};

export function useSmoothCornerClipPath<T extends HTMLElement>({
  cornerRadius,
  cornerSmoothing = 0.6,
}: UseSmoothCornerClipPathOptions) {
  const ref = React.useRef<T | null>(null);
  const [clipPath, setClipPath] = React.useState<string | null>(null);

  React.useEffect(() => {
    const node = ref.current;
    if (!node) {
      return;
    }

    const updateClipPath = () => {
      const width = node.offsetWidth;
      const height = node.offsetHeight;
      if (width <= 0 || height <= 0) {
        setClipPath(null);
        return;
      }

      setClipPath(
        getSmoothCornerPath({
          cornerRadius,
          cornerSmoothing,
          height,
          width,
        }),
      );
    };

    updateClipPath();

    const observer = new ResizeObserver(updateClipPath);
    observer.observe(node);

    return () => {
      observer.disconnect();
    };
  }, [cornerRadius, cornerSmoothing]);

  return {
    ref,
    style: {
      borderRadius: `${cornerRadius}px`,
      clipPath: clipPath ? `path("${clipPath}")` : undefined,
      WebkitClipPath: clipPath ? `path("${clipPath}")` : undefined,
    } satisfies React.CSSProperties,
  };
}
