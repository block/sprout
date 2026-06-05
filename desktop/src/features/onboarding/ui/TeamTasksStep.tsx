import * as React from "react";
import { Send } from "lucide-react";

import { cn } from "@/shared/lib/cn";
import { rewriteRelayUrl } from "@/shared/lib/mediaUrl";
import { DEFAULT_AGENTS } from "./defaultAgents";
import type { OnboardingProfileValues } from "./types";

const TEAM_AGENTS = DEFAULT_AGENTS.slice(0, 2);
const INITIAL_PROMPT =
  "Hey Kenny, do you know anything about the marketing plan going out next week?";
const DEFAULT_REPLY = "Can Kit and Scout take a look and tell me what matters?";
const COMPOSER_PLACEHOLDER = "Try asking Kit and Scout about the plan.";
const THINKING_MS = 1500;
const KIT_RESPONSE_MS = 520;
const SCOUT_RESPONSE_MS = 620;

type DemoStage = "idle" | "thinking" | "team" | "kit" | "scout";

type DemoResponse = {
  acknowledgement: string;
  agentMessages: [string, string];
};

const MARKETING_PLAN_TERMS = [
  "marketing",
  "plan",
  "launch",
  "campaign",
  "budget",
  "next week",
  "promo",
  "copy",
  "email",
  "social",
  "creator",
  "landing page",
  "owner",
  "risk",
  "approval",
  "schedule",
  "timeline",
];
const HANDOFF_TERMS = [
  "i don't know",
  "i dont know",
  "do not know",
  "don't know",
  "dont know",
  "not sure",
  "unsure",
  "no idea",
  "no clue",
  "can you check",
  "can you look",
  "can you find",
  "take a look",
  "please check",
  "help me",
];
const OFF_TOPIC_TERMS = [
  "restaurant",
  "dinner",
  "lunch",
  "weather",
  "flight",
  "hotel",
  "vacation",
  "calendar",
  "movie",
  "recipe",
  "workout",
];

function isMarketingPlanPrompt(prompt: string) {
  const normalizedPrompt = prompt.toLowerCase();
  return MARKETING_PLAN_TERMS.some((term) => normalizedPrompt.includes(term));
}

function isHandoffPrompt(prompt: string) {
  const normalizedPrompt = prompt.toLowerCase();
  return HANDOFF_TERMS.some((term) => normalizedPrompt.includes(term));
}

function isClearlyOffTopicPrompt(prompt: string) {
  const normalizedPrompt = prompt.toLowerCase();
  return OFF_TOPIC_TERMS.some((term) => normalizedPrompt.includes(term));
}

function buildDemoResponse(prompt: string): DemoResponse {
  const normalizedPrompt = prompt.toLowerCase();

  if (!isMarketingPlanPrompt(prompt) && isHandoffPrompt(prompt)) {
    return {
      acknowledgement: "No worries, we'll check the plan.",
      agentMessages: [
        "I'll pull together the marketing plan going out next week. The launch is staged around email, paid social, and creator posts feeding one landing page.",
        "I'll review the open questions while Kit organizes it: promo-copy approval, channel owners, and anything that could block next Wednesday.",
      ],
    };
  }

  if (!isMarketingPlanPrompt(prompt) && isClearlyOffTopicPrompt(prompt)) {
    return {
      acknowledgement: "We'll bring this back to the marketing plan.",
      agentMessages: [
        "I don't know anything about that yet, but I can check the marketing plan going out next week. The launch still points to email, paid social, and creator posts feeding one landing page.",
        "I'll keep the review anchored there. First checkpoint: confirm promo-copy approval and the owner for each launch channel before next Wednesday.",
      ],
    };
  }

  if (!isMarketingPlanPrompt(prompt)) {
    return {
      acknowledgement: "We'll use the marketing plan as the thread.",
      agentMessages: [
        "I'll treat this as a request to check the plan Jamie mentioned. The launch is staged for next Wednesday across email, paid social, and creator posts.",
        "I'll review the plan for risks and owners so the answer stays useful: copy approval, launch timing, and channel accountability.",
      ],
    };
  }

  if (
    normalizedPrompt.includes("budget") ||
    normalizedPrompt.includes("spend")
  ) {
    return {
      acknowledgement: "We're on the budget thread.",
      agentMessages: [
        "I found the budget path. Paid social is the only channel that needs a final spend cap before the launch checklist is ready.",
        "I'll review the owner map for that approval and flag any channel without a named decision maker before next Wednesday.",
      ],
    };
  }

  if (
    normalizedPrompt.includes("copy") ||
    normalizedPrompt.includes("promo") ||
    normalizedPrompt.includes("email")
  ) {
    return {
      acknowledgement: "We're checking the copy path.",
      agentMessages: [
        "Found it. Email, paid social, and creator posts all point to the same landing page, so the launch checklist can stay centered on one message.",
        "I found one open risk: final approval on the promo copy. Kit is turning the plan into a launch-day checklist while I confirm the owners.",
      ],
    };
  }

  if (
    normalizedPrompt.includes("owner") ||
    normalizedPrompt.includes("risk") ||
    normalizedPrompt.includes("approval")
  ) {
    return {
      acknowledgement: "We're checking risks and owners.",
      agentMessages: [
        "I mapped the plan into owners: lifecycle for email, growth for paid social, and brand for creator posts. The launch still targets next Wednesday.",
        "I found one open risk: promo-copy approval is not locked yet. I'll confirm the approver and flag anything missing from the owner list.",
      ],
    };
  }

  if (
    normalizedPrompt.includes("schedule") ||
    normalizedPrompt.includes("timeline") ||
    normalizedPrompt.includes("next week")
  ) {
    return {
      acknowledgement: "We're checking the launch timing.",
      agentMessages: [
        "Found it. The plan is staged for next Wednesday, with email first, paid social later that morning, and creator posts queued after the landing page check.",
        "I'll review timing dependencies and watch the copy approval, because that is the one item that could push the schedule.",
      ],
    };
  }

  return {
    acknowledgement: "We're on it.",
    agentMessages: [
      "Found it. The launch plan is staged for next Wednesday with email, paid social, and creator posts all pointing to the same landing page.",
      "I found one open risk: final approval on the promo copy. Kit is turning the plan into a launch-day checklist while I confirm the owners.",
    ],
  };
}

type TeamThreadMessageProps = {
  align?: "left" | "right";
  avatar: React.ReactNode;
  bubbleClassName?: string;
  children: React.ReactNode;
  className?: string;
  label: string;
  meta?: string;
};

function TeamThreadMessage({
  align = "left",
  avatar,
  bubbleClassName,
  children,
  className,
  label,
  meta,
}: TeamThreadMessageProps) {
  const isRightAligned = align === "right";

  return (
    <div
      className={cn(
        "flex items-start gap-3",
        isRightAligned && "flex-row-reverse",
        className,
      )}
    >
      {avatar}

      <div
        className={cn(
          "min-w-0 max-w-[560px]",
          isRightAligned && "flex flex-col items-end",
        )}
      >
        <div
          className={cn(
            "arcade-type-label-extra-small mb-2 flex items-center gap-2 uppercase text-[var(--arcade-text-subtle)]",
            isRightAligned && "justify-end",
          )}
        >
          <span>{label}</span>
          {meta ? <span className="text-black/35">{meta}</span> : null}
        </div>

        <div
          className={cn(
            "arcade-type-body-small w-fit max-w-full break-words px-4 py-3 text-black shadow-[0_1px_0_rgba(0,0,0,0.04)]",
            isRightAligned
              ? "rounded-[30px] rounded-tr-[8px] bg-black text-white"
              : "rounded-[30px] rounded-tl-[8px] bg-white",
            bubbleClassName,
          )}
        >
          {children}
        </div>
      </div>
    </div>
  );
}

function JamieAvatar() {
  return (
    <div
      aria-label="Jamie avatar"
      className="flex h-12 w-12 shrink-0 items-center justify-center rounded-full border border-[var(--arcade-semantic-border-subtle)] bg-[#FF5A7A] text-[25px] shadow-[inset_0_-2px_0_rgba(0,0,0,0.12)]"
      role="img"
    >
      🙂
    </div>
  );
}

type UserDemoAvatarProps = {
  avatarUrl: string;
  displayName: string;
};

function UserDemoAvatar({ avatarUrl, displayName }: UserDemoAvatarProps) {
  const [failedAvatarUrl, setFailedAvatarUrl] = React.useState<string | null>(
    null,
  );
  const trimmedAvatarUrl = avatarUrl.trim();
  const initial = displayName.trim().charAt(0).toUpperCase() || "K";
  const shouldShowImage =
    trimmedAvatarUrl.length > 0 && failedAvatarUrl !== trimmedAvatarUrl;

  return (
    <div className="flex h-12 w-12 shrink-0 items-center justify-center overflow-hidden rounded-full border border-[var(--arcade-semantic-border-subtle)] bg-black text-[18px] font-semibold text-white shadow-[0_1px_0_rgba(0,0,0,0.08)]">
      {shouldShowImage ? (
        <img
          alt={`${displayName} avatar`}
          className="h-full w-full object-cover"
          draggable={false}
          onError={() => setFailedAvatarUrl(trimmedAvatarUrl)}
          src={rewriteRelayUrl(trimmedAvatarUrl)}
        />
      ) : (
        initial
      )}
    </div>
  );
}

type AgentAvatarProps = {
  agent: (typeof DEFAULT_AGENTS)[number];
};

function AgentAvatar({ agent }: AgentAvatarProps) {
  return (
    <div className="flex h-12 w-12 shrink-0 items-center justify-center overflow-hidden rounded-full border border-[var(--arcade-semantic-border-subtle)] bg-white shadow-[0_1px_0_rgba(0,0,0,0.08)]">
      <video
        aria-label={agent.mediaLabel}
        autoPlay
        className="h-9 w-9 object-contain"
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
}

function TeamAvatarStack() {
  return (
    <div className="flex h-12 w-[76px] shrink-0 items-center">
      {TEAM_AGENTS.map((agent, index) => (
        <div
          className={cn(
            "flex h-12 w-12 items-center justify-center overflow-hidden rounded-full border border-[var(--arcade-semantic-border-subtle)] bg-white shadow-[0_1px_0_rgba(0,0,0,0.08)]",
            index > 0 && "-ml-5",
          )}
          key={agent.name}
          style={{ zIndex: TEAM_AGENTS.length - index }}
        >
          <video
            aria-label={agent.mediaLabel}
            autoPlay
            className="h-9 w-9 object-contain"
            loop
            muted
            playsInline
            poster={agent.mediaPng}
            preload="metadata"
          >
            <source src={agent.mediaMp4} type="video/mp4" />
          </video>
        </div>
      ))}
    </div>
  );
}

function ThinkingDots() {
  return (
    <span className="flex h-6 w-[26px] items-center justify-center gap-1">
      <span className="sr-only">Kit and Scout are thinking</span>
      {[0, 1, 2].map((dot) => (
        <span
          aria-hidden="true"
          className="sprout-onboarding-thinking-dot h-1.5 w-1.5 rounded-full bg-black/45"
          key={dot}
          style={{ animationDelay: `${dot * 120}ms` }}
        />
      ))}
    </span>
  );
}

type TeamTasksStepProps = {
  profile: OnboardingProfileValues;
};

export function TeamTasksStep({ profile }: TeamTasksStepProps) {
  const [draft, setDraft] = React.useState("");
  const [submittedPrompt, setSubmittedPrompt] = React.useState<string | null>(
    null,
  );
  const [demoStage, setDemoStage] = React.useState<DemoStage>("idle");
  const thinkingTimerRef = React.useRef<number | null>(null);
  const kitTimerRef = React.useRef<number | null>(null);
  const scoutTimerRef = React.useRef<number | null>(null);
  const displayName = profile.displayName.trim() || "Kenny";
  const demoResponse = React.useMemo(
    () => buildDemoResponse(submittedPrompt ?? DEFAULT_REPLY),
    [submittedPrompt],
  );
  const hasSubmittedPrompt = submittedPrompt !== null;
  const isThinking = demoStage === "thinking";
  const hasTeamResponse =
    demoStage === "team" || demoStage === "kit" || demoStage === "scout";
  const hasKitResponse = demoStage === "kit" || demoStage === "scout";
  const hasScoutResponse = demoStage === "scout";

  React.useEffect(() => {
    return () => {
      if (thinkingTimerRef.current !== null) {
        window.clearTimeout(thinkingTimerRef.current);
      }
      if (kitTimerRef.current !== null) {
        window.clearTimeout(kitTimerRef.current);
      }
      if (scoutTimerRef.current !== null) {
        window.clearTimeout(scoutTimerRef.current);
      }
    };
  }, []);

  const runDemo = React.useCallback(
    (event?: React.FormEvent<HTMLFormElement>) => {
      event?.preventDefault();

      if (thinkingTimerRef.current !== null) {
        window.clearTimeout(thinkingTimerRef.current);
      }
      if (kitTimerRef.current !== null) {
        window.clearTimeout(kitTimerRef.current);
      }
      if (scoutTimerRef.current !== null) {
        window.clearTimeout(scoutTimerRef.current);
      }

      setSubmittedPrompt(draft.trim() || DEFAULT_REPLY);
      setDraft("");
      setDemoStage("thinking");
      thinkingTimerRef.current = window.setTimeout(() => {
        thinkingTimerRef.current = null;
        setDemoStage("team");
        kitTimerRef.current = window.setTimeout(() => {
          kitTimerRef.current = null;
          setDemoStage("kit");
          scoutTimerRef.current = window.setTimeout(() => {
            scoutTimerRef.current = null;
            setDemoStage("scout");
          }, SCOUT_RESPONSE_MS);
        }, KIT_RESPONSE_MS);
      }, THINKING_MS);
    },
    [draft],
  );

  return (
    <div
      className="relative flex h-[760px] w-full max-w-[760px] flex-col overflow-hidden"
      data-testid="onboarding-page-team"
    >
      <h1 className="arcade-type-headline-small text-[var(--arcade-text-standard)]">
        #Marketing-budget2026
      </h1>

      <div className="mt-8 flex min-h-0 flex-1 flex-col rounded-[8px] bg-[#F2F2F2] p-5 shadow-[0_1px_0_rgba(0,0,0,0.04)]">
        <section
          aria-label="Interactive agent team conversation demo"
          className="min-h-0 flex-1 overflow-hidden"
        >
          <div className="flex h-full flex-col gap-5 overflow-y-auto pr-1">
            <TeamThreadMessage
              avatar={<JamieAvatar />}
              className="sprout-onboarding-demo-message sprout-onboarding-demo-message-1"
              label="Jamie"
              meta="Marketing"
            >
              {INITIAL_PROMPT}
            </TeamThreadMessage>

            {submittedPrompt ? (
              <TeamThreadMessage
                align="right"
                avatar={
                  <UserDemoAvatar
                    avatarUrl={profile.avatarUrl}
                    displayName={displayName}
                  />
                }
                className="sprout-onboarding-demo-message sprout-onboarding-demo-message-1"
                label={displayName}
              >
                {submittedPrompt}
              </TeamThreadMessage>
            ) : null}

            {hasSubmittedPrompt ? (
              <>
                <TeamThreadMessage
                  avatar={<TeamAvatarStack />}
                  bubbleClassName={
                    isThinking || !hasTeamResponse ? "px-3 py-2" : undefined
                  }
                  className="sprout-onboarding-demo-message sprout-onboarding-demo-message-2"
                  label="Kit + Scout"
                  meta="Team"
                >
                  {isThinking || !hasTeamResponse ? (
                    <ThinkingDots />
                  ) : (
                    <p>{demoResponse.acknowledgement}</p>
                  )}
                </TeamThreadMessage>

                {hasKitResponse ? (
                  <div className="grid gap-3 pl-[88px]">
                    {TEAM_AGENTS.slice(0, hasScoutResponse ? 2 : 1).map(
                      (agent, index) => (
                        <TeamThreadMessage
                          avatar={<AgentAvatar agent={agent} />}
                          className={cn(
                            index === 0
                              ? "sprout-onboarding-demo-message sprout-onboarding-demo-message-3"
                              : "sprout-onboarding-demo-message sprout-onboarding-demo-message-4",
                          )}
                          key={agent.name}
                          label={agent.name}
                          meta={agent.role}
                        >
                          {demoResponse.agentMessages[index]}
                        </TeamThreadMessage>
                      ),
                    )}
                  </div>
                ) : null}
              </>
            ) : null}
          </div>
        </section>

        <form className="mt-5 flex items-center gap-3" onSubmit={runDemo}>
          <label className="sr-only" htmlFor="team-task-demo-input">
            Ask Kit and Scout
          </label>
          <input
            className="arcade-type-body-small h-14 min-w-0 flex-1 rounded-full border-0 bg-white px-5 text-black shadow-none outline-none transition-colors placeholder:italic placeholder:text-black/35 focus:bg-white"
            id="team-task-demo-input"
            onChange={(event) => setDraft(event.target.value)}
            placeholder={COMPOSER_PLACEHOLDER}
            type="text"
            value={draft}
          />
          <button
            aria-label="Send demo message"
            className="flex h-14 w-14 shrink-0 items-center justify-center rounded-full bg-black text-white transition-[background-color,transform] duration-200 ease-out hover:bg-black/85 active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-black/25"
            type="submit"
          >
            <Send className="h-5 w-5" strokeWidth={2.25} />
          </button>
        </form>
      </div>
    </div>
  );
}
