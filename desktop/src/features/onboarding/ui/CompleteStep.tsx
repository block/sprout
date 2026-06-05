import * as React from "react";

import { useEmojiBurst } from "@/shared/ui/EmojiBurstProvider";
import { playOnboardingSound } from "./onboardingSounds";

export function CompleteStep() {
  const { celebrateWithEmojiFloatBurst } = useEmojiBurst();
  const didCelebrateRef = React.useRef(false);

  React.useEffect(() => {
    if (didCelebrateRef.current) {
      return;
    }

    didCelebrateRef.current = true;
    playOnboardingSound("tada");
    celebrateWithEmojiFloatBurst();
  }, [celebrateWithEmojiFloatBurst]);

  return (
    <div
      className="relative flex h-[704px] w-full items-center justify-center overflow-visible"
      data-testid="onboarding-page-complete"
    >
      <img
        alt="Sprout cake"
        className="sprout-complete-cake relative z-20 w-[420px] max-w-[48vw] select-none object-contain"
        draggable={false}
        src="/onboarding/cake.png"
      />
    </div>
  );
}
