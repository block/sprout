import * as React from "react";

import { DEFAULT_AGENTS } from "./defaultAgents";
import { playOnboardingSound } from "./onboardingSounds";

function getCarouselOffset(index: number, activeIndex: number) {
  return index - activeIndex;
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

export function AgentsStep() {
  const [activeIndex, setActiveIndex] = React.useState(0);
  const isFirstAgent = activeIndex === 0;
  const isLastAgent = activeIndex === DEFAULT_AGENTS.length - 1;

  const showPreviousAgent = React.useCallback(() => {
    setActiveIndex((currentIndex) => Math.max(currentIndex - 1, 0));
  }, []);

  const showNextAgent = React.useCallback(() => {
    setActiveIndex((currentIndex) =>
      Math.min(currentIndex + 1, DEFAULT_AGENTS.length - 1),
    );
  }, []);

  const playPreviousAgentSound = React.useCallback(() => {
    if (!isFirstAgent) {
      playOnboardingSound("minorB");
    }
  }, [isFirstAgent]);

  const playNextAgentSound = React.useCallback(() => {
    if (!isLastAgent) {
      playOnboardingSound("minorA");
    }
  }, [isLastAgent]);

  const playAgentChevronSoundOnKeyboardPress = React.useCallback(
    (
      event: React.KeyboardEvent<HTMLButtonElement>,
      direction: "next" | "previous",
    ) => {
      if (event.repeat || (event.key !== "Enter" && event.key !== " ")) {
        return;
      }

      if (direction === "previous") {
        playPreviousAgentSound();
        return;
      }

      playNextAgentSound();
    },
    [playNextAgentSound, playPreviousAgentSound],
  );

  React.useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) {
        return;
      }

      if (event.key === "ArrowLeft") {
        event.preventDefault();
        playPreviousAgentSound();
        showPreviousAgent();
        return;
      }

      if (event.key === "ArrowRight") {
        event.preventDefault();
        playNextAgentSound();
        showNextAgent();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [
    playNextAgentSound,
    playPreviousAgentSound,
    showNextAgent,
    showPreviousAgent,
  ]);

  return (
    <div
      className="flex w-full max-w-[828px] flex-col items-center"
      data-testid="onboarding-page-agents"
    >
      <div className="flex flex-col items-center text-center">
        <h1 className="arcade-type-headline-small text-[var(--arcade-text-standard)]">
          Meet the default agents
        </h1>
      </div>

      <div className="relative mt-10 flex w-full flex-col items-center justify-center">
        <div className="relative h-[548px] w-full max-w-[720px] overflow-visible">
          <div
            className="pointer-events-none absolute left-1/2 top-[45%] flex h-[500px] w-[316px] -translate-x-1/2 -translate-y-1/2 flex-col overflow-visible rounded-[8px] bg-[var(--arcade-semantic-background-prominent)] p-4"
            data-active="true"
          >
            <div className="relative flex h-[300px] w-full items-center justify-center overflow-visible rounded-[4px] bg-white">
              <div className="absolute left-1/2 top-1/2 h-[216px] w-[620px] -translate-x-1/2 -translate-y-1/2 overflow-visible">
                {DEFAULT_AGENTS.map((agent, index) => {
                  const carouselOffset = getCarouselOffset(index, activeIndex);
                  const isActive = carouselOffset === 0;
                  const carouselDistance = Math.abs(carouselOffset);
                  const carouselDirection = Math.sign(carouselOffset);
                  const avatarTranslateX =
                    carouselDistance === 0
                      ? 0
                      : carouselDirection *
                        (carouselDistance === 1 ? 242 : 380);

                  return (
                    <div
                      aria-hidden={!isActive}
                      className="absolute left-1/2 top-1/2 flex h-[208px] w-[208px] items-center justify-center transition-[opacity,transform,filter] duration-500 ease-[cubic-bezier(0.22,1,0.36,1)]"
                      key={agent.name}
                      style={{
                        filter: isActive ? "none" : "saturate(0.86)",
                        opacity: isActive
                          ? 1
                          : carouselDistance === 1
                            ? 0.42
                            : 0.22,
                        transform: `translate(-50%, -50%) translateX(${
                          avatarTranslateX
                        }px) scale(${
                          isActive ? 1 : carouselDistance === 1 ? 0.58 : 0.42
                        })`,
                        zIndex: isActive ? 2 : 1,
                      }}
                    >
                      <video
                        aria-label={agent.mediaLabel}
                        autoPlay
                        className="h-full w-full object-contain"
                        loop
                        muted
                        playsInline
                        poster={agent.mediaPng}
                        preload="metadata"
                      >
                        <source src={agent.mediaMp4} type="video/mp4" />
                      </video>
                    </div>
                  );
                })}
              </div>
            </div>

            <div className="relative mt-7 flex flex-1 flex-col overflow-hidden text-left">
              {DEFAULT_AGENTS.map((agent, index) => {
                const isActive = activeIndex === index;
                const carouselOffset = getCarouselOffset(index, activeIndex);

                return (
                  <div
                    aria-hidden={!isActive}
                    className="absolute inset-0 flex flex-col transition-[opacity,transform] duration-300 ease-[cubic-bezier(0.22,1,0.36,1)]"
                    key={agent.name}
                    style={{
                      opacity: isActive ? 1 : 0,
                      transform: isActive
                        ? "translateX(0)"
                        : `translateX(${carouselOffset < 0 ? -18 : 18}px)`,
                    }}
                  >
                    <h2 className="arcade-type-page-title-display text-[var(--arcade-text-standard)]">
                      {agent.name}
                    </h2>
                    <div className="arcade-type-label-extra-small mt-1 uppercase text-[var(--arcade-text-subtle)]">
                      {agent.role}
                    </div>
                    <div className="absolute inset-x-0 bottom-0 flex flex-wrap gap-2">
                      {agent.skills.map((skill) => (
                        <span
                          className="arcade-type-label-extra-small rounded-[2px] bg-white px-3 py-2 uppercase text-black"
                          key={skill}
                        >
                          {skill}
                        </span>
                      ))}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        </div>

        <div className="mt-0 flex items-center justify-center gap-4">
          <button
            aria-label="Previous default agent"
            className="flex h-14 w-14 items-center justify-center rounded-full bg-transparent text-black/60 transition-[background-color,color,transform] duration-200 ease-out hover:bg-white hover:text-black active:scale-95 disabled:pointer-events-none disabled:opacity-30 focus-visible:bg-white focus-visible:text-black focus-visible:outline-none"
            disabled={isFirstAgent}
            onClick={showPreviousAgent}
            onKeyDown={(event) =>
              playAgentChevronSoundOnKeyboardPress(event, "previous")
            }
            onPointerDown={playPreviousAgentSound}
            type="button"
          >
            <ArcadeCaretDownIcon className="h-7 w-7 rotate-90" />
          </button>

          <button
            aria-label="Next default agent"
            className="flex h-14 w-14 items-center justify-center rounded-full bg-transparent text-black/60 transition-[background-color,color,transform] duration-200 ease-out hover:bg-white hover:text-black active:scale-95 disabled:pointer-events-none disabled:opacity-30 focus-visible:bg-white focus-visible:text-black focus-visible:outline-none"
            disabled={isLastAgent}
            onClick={showNextAgent}
            onKeyDown={(event) =>
              playAgentChevronSoundOnKeyboardPress(event, "next")
            }
            onPointerDown={playNextAgentSound}
            type="button"
          >
            <ArcadeCaretDownIcon className="h-7 w-7 -rotate-90" />
          </button>
        </div>
      </div>
    </div>
  );
}
