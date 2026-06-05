import * as React from "react";

import { cn } from "@/shared/lib/cn";
import {
  playOnboardingSound,
  playOnboardingTypingSoundForKey,
} from "./onboardingSounds";
import type { ProfileStepActions, ProfileStepState } from "./types";

type ProfileStepProps = {
  actions: ProfileStepActions;
  state: ProfileStepState;
};

function ErrorBanner({ message }: { message: string | null }) {
  if (!message) {
    return null;
  }

  return (
    <p className="arcade-type-body-medium mt-4 rounded-[8px] bg-white px-4 py-3 text-destructive">
      {message}
    </p>
  );
}

export function ProfileStep({ actions, state }: ProfileStepProps) {
  const { submit, updateDisplayName } = actions;
  const { isSaving, name, saveRecovery } = state;
  const displayNameDraft = name.draftValue;
  const hasDisplayNameDraft = displayNameDraft.length > 0;
  const canSubmit = displayNameDraft.trim().length > 0 && !isSaving;
  const [isNameFieldReady, setIsNameFieldReady] = React.useState(false);
  const inputRef = React.useRef<HTMLInputElement>(null);

  React.useLayoutEffect(() => {
    let animationFrame: number | null = null;

    inputRef.current?.focus();
    animationFrame = window.requestAnimationFrame(() => {
      setIsNameFieldReady(true);
    });

    return () => {
      if (animationFrame !== null) {
        window.cancelAnimationFrame(animationFrame);
      }
    };
  }, []);

  return (
    <label
      className="flex h-full w-full cursor-text flex-col items-center justify-center border-0 bg-transparent p-0 text-inherit"
      data-testid="onboarding-page-1"
      htmlFor="onboarding-display-name"
    >
      <span className="sr-only">Name</span>
      <div
        className={cn(
          "relative h-20 w-full max-w-[576px] origin-center transition-[opacity,transform] duration-300 ease-[cubic-bezier(0.16,1,0.3,1)]",
          isNameFieldReady
            ? "translate-y-0 scale-100 opacity-100"
            : "translate-y-3 scale-[0.8] opacity-0",
        )}
      >
        {!hasDisplayNameDraft ? (
          <div
            aria-hidden="true"
            className="pointer-events-none absolute inset-0 flex select-none items-center justify-center"
          >
            <span className="sprout-name-placeholder-caret arcade-type-numeral-large relative select-none text-[#8E8E8E]">
              Enter your name
            </span>
          </div>
        ) : null}
        <input
          aria-label="Name"
          className={cn(
            "arcade-type-numeral-large h-full w-full border-0 bg-transparent px-0 py-0 text-center shadow-none outline-none disabled:cursor-not-allowed disabled:opacity-50",
            hasDisplayNameDraft
              ? "text-[var(--arcade-text-standard)] caret-[var(--arcade-text-standard)]"
              : "text-transparent caret-transparent",
          )}
          data-testid="onboarding-display-name"
          disabled={isSaving}
          id="onboarding-display-name"
          onChange={(event) => updateDisplayName(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && canSubmit) {
              event.preventDefault();
              playOnboardingSound("toggleA");
              submit();
              return;
            }

            playOnboardingTypingSoundForKey(event);
          }}
          ref={inputRef}
          value={displayNameDraft}
        />
      </div>
      <ErrorBanner message={saveRecovery.errorMessage} />
    </label>
  );
}
