import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import type { AgentPersona } from "@/shared/api/types";
import { promptPreview } from "@/shared/lib/promptPreview";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/shared/ui/sheet";

import { PersonaCatalogSelectionBadge } from "./PersonaCatalogSelectionBadge";
import { PersonaCatalogSelectionControl } from "./PersonaCatalogSelectionControl";
import { getPersonaCatalogDetailSelectionCopy } from "./personaLibraryCopy";

type PersonaCatalogDetailsSheetProps = {
  feedbackErrorMessage: string | null;
  feedbackNoticeMessage: string | null;
  isPending: boolean;
  onOpenChange: (open: boolean) => void;
  onSelectPersona: (persona: AgentPersona, active: boolean) => void;
  open: boolean;
  persona: AgentPersona | null;
};

export function PersonaCatalogDetailsSheet({
  feedbackErrorMessage,
  feedbackNoticeMessage,
  isPending,
  onOpenChange,
  onSelectPersona,
  open,
  persona,
}: PersonaCatalogDetailsSheetProps) {
  const preview = persona ? promptPreview(persona.systemPrompt) : "";
  const selectionCopy = getPersonaCatalogDetailSelectionCopy(
    persona?.isActive ?? false,
  );

  return (
    <Sheet onOpenChange={onOpenChange} open={open}>
      <SheetContent
        className="w-full overflow-y-auto sm:max-w-xl"
        data-testid="persona-catalog-details-sheet"
      >
        {persona ? (
          <div className="space-y-6 pr-4">
            <SheetHeader className="border-b border-border/60 pb-4 pr-10">
              <div className="flex items-start gap-3">
                <ProfileAvatar
                  avatarUrl={persona.avatarUrl}
                  className="h-12 w-12 rounded-xl text-sm"
                  label={persona.displayName}
                />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <SheetTitle className="truncate text-xl">
                      {persona.displayName}
                    </SheetTitle>
                    <PersonaCatalogSelectionBadge isActive={persona.isActive} />
                  </div>
                  <SheetDescription className="mt-2">
                    {preview || "No summary available."}
                  </SheetDescription>
                </div>
              </div>
            </SheetHeader>

            <div className="rounded-xl border border-border/70 bg-card/70 p-4">
              <div className="flex items-start justify-between gap-4">
                <div>
                  <p
                    className="text-sm font-semibold tracking-tight"
                    data-testid="persona-catalog-detail-selection-title"
                  >
                    {selectionCopy.title}
                  </p>
                  <p
                    className="mt-1 text-sm text-muted-foreground"
                    data-testid="persona-catalog-detail-selection-description"
                  >
                    {selectionCopy.description}
                  </p>
                </div>
                <PersonaCatalogSelectionControl
                  isPending={isPending}
                  onCheckedChange={(checked) => {
                    onSelectPersona(persona, checked === true);
                  }}
                  persona={persona}
                  variant="detail"
                />
              </div>
            </div>

            {feedbackNoticeMessage ? (
              <p className="rounded-2xl border border-primary/20 bg-primary/10 px-4 py-3 text-sm text-primary">
                {feedbackNoticeMessage}
              </p>
            ) : null}

            {feedbackErrorMessage ? (
              <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {feedbackErrorMessage}
              </p>
            ) : null}

            <div className="grid gap-3 sm:grid-cols-2">
              <div className="rounded-xl border border-border/70 bg-card/70 p-4">
                <p className="text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground">
                  Type
                </p>
                <p className="mt-2 text-sm font-medium">Built-in persona</p>
              </div>
              <div className="rounded-xl border border-border/70 bg-card/70 p-4">
                <p className="text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground">
                  Preferred model
                </p>
                <p className="mt-2 text-sm font-medium">
                  {persona.model ?? "Use app default"}
                </p>
              </div>
              <div className="rounded-xl border border-border/70 bg-card/70 p-4 sm:col-span-2">
                <p className="text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground">
                  Preferred provider
                </p>
                <p className="mt-2 text-sm font-medium">
                  {persona.provider ?? "Use app default"}
                </p>
              </div>
            </div>

            <div className="rounded-xl border border-border/70 bg-card/70 p-4">
              <p className="text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground">
                System prompt
              </p>
              <pre className="mt-3 whitespace-pre-wrap break-words font-sans text-sm leading-6 text-foreground">
                {persona.systemPrompt}
              </pre>
            </div>
          </div>
        ) : null}
      </SheetContent>
    </Sheet>
  );
}
