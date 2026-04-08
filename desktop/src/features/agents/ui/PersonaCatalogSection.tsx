import type { CheckedState } from "@radix-ui/react-checkbox";

import type { AgentPersona } from "@/shared/api/types";
import { promptPreview } from "@/shared/lib/promptPreview";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Checkbox } from "@/shared/ui/checkbox";
import { Skeleton } from "@/shared/ui/skeleton";
import {
  getPersonaCatalogToggleAriaLabel,
  PersonaCatalogSelectionBadge,
} from "./PersonaCatalogSelectionBadge";
import { PersonaIdentity } from "./PersonaIdentity";
import { personaCatalogCopy } from "./personaLibraryCopy";

type PersonaCatalogSectionProps = {
  emptyDescription?: string;
  emptyTitle?: string;
  error: Error | null;
  feedbackErrorMessage?: string | null;
  feedbackNoticeMessage?: string | null;
  isLoading: boolean;
  isPending: boolean;
  onSelectPersona: (persona: AgentPersona, active: boolean) => void;
  onViewDetails: (persona: AgentPersona) => void;
  personas: AgentPersona[];
  showHeader?: boolean;
};

export function PersonaCatalogSection({
  emptyDescription = personaCatalogCopy.emptyCatalogDescription,
  emptyTitle = personaCatalogCopy.emptyCatalogTitle,
  error,
  feedbackErrorMessage = null,
  feedbackNoticeMessage = null,
  isLoading,
  isPending,
  onSelectPersona,
  onViewDetails,
  personas,
  showHeader = true,
}: PersonaCatalogSectionProps) {
  function handleCheckedChange(persona: AgentPersona, checked: CheckedState) {
    onSelectPersona(persona, checked === true);
  }

  return (
    <section className="space-y-4" data-testid="agents-persona-catalog">
      {showHeader ? (
        <div>
          <h3 className="text-sm font-semibold tracking-tight">
            {personaCatalogCopy.title}
          </h3>
          <p className="text-sm text-muted-foreground">
            {personaCatalogCopy.description}
          </p>
        </div>
      ) : null}

      {isLoading ? (
        <div className="grid gap-3 md:grid-cols-3 xl:grid-cols-4">
          {["first", "second", "third", "fourth"].map((key) => (
            <div
              className="rounded-xl border border-border/70 bg-card/80 p-3 shadow-sm"
              key={key}
            >
              <div className="flex items-center gap-2.5">
                <Skeleton className="h-8 w-8 rounded-lg" />
                <div className="space-y-2">
                  <Skeleton className="h-4 w-24" />
                  <Skeleton className="h-3 w-20" />
                </div>
              </div>
            </div>
          ))}
        </div>
      ) : null}

      {!isLoading && personas.length > 0 ? (
        <div className="grid gap-3 md:grid-cols-3 xl:grid-cols-4">
          {personas.map((persona) => {
            const preview = promptPreview(persona.systemPrompt);
            const toggleId = `persona-catalog-toggle-control-${persona.id}`;

            return (
              <div
                className={cn(
                  "flex flex-col gap-4 rounded-xl border bg-card/80 p-3 shadow-sm transition-colors",
                  persona.isActive
                    ? "border-primary/40 bg-primary/[0.04]"
                    : "border-border/70",
                )}
                data-testid={`persona-catalog-card-${persona.id}`}
                key={persona.id}
              >
                <div className="flex items-start justify-between gap-3">
                  <PersonaIdentity
                    className="min-w-0 flex-1"
                    persona={persona}
                    showBuiltInBadge={false}
                    showPromptTooltip={false}
                  />

                  <PersonaCatalogSelectionBadge isActive={persona.isActive} />
                </div>

                <p className="min-h-12 text-xs leading-5 text-muted-foreground">
                  {preview}
                </p>

                <div className="mt-auto flex items-center justify-between gap-3 border-t border-border/60 pt-3">
                  <Button
                    data-testid={`persona-catalog-details-${persona.id}`}
                    onClick={() => onViewDetails(persona)}
                    size="sm"
                    type="button"
                    variant="ghost"
                  >
                    {personaCatalogCopy.detailsAction}
                  </Button>

                  <label
                    className={cn(
                      "flex items-center gap-2 rounded-md px-2 py-1 text-xs font-medium text-foreground transition-colors",
                      isPending
                        ? "cursor-not-allowed opacity-70"
                        : "cursor-pointer hover:bg-muted/60",
                    )}
                    data-testid={`persona-catalog-toggle-target-${persona.id}`}
                    htmlFor={toggleId}
                  >
                    <Checkbox
                      aria-label={getPersonaCatalogToggleAriaLabel(
                        persona.displayName,
                      )}
                      checked={persona.isActive}
                      data-testid={`persona-catalog-toggle-${persona.id}`}
                      disabled={isPending}
                      id={toggleId}
                      onCheckedChange={(checked) =>
                        handleCheckedChange(persona, checked)
                      }
                    />
                    <span>
                      {persona.isActive
                        ? personaCatalogCopy.deselectAction
                        : personaCatalogCopy.selectAction}
                    </span>
                  </label>
                </div>
              </div>
            );
          })}
        </div>
      ) : null}

      {!isLoading && personas.length === 0 ? (
        <div className="rounded-xl border border-dashed border-border/70 px-6 py-10 text-center">
          <p className="text-sm font-semibold tracking-tight">{emptyTitle}</p>
          <p className="mt-2 text-sm text-muted-foreground">
            {emptyDescription}
          </p>
        </div>
      ) : null}

      {error ? (
        <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {error.message}
        </p>
      ) : null}

      {feedbackNoticeMessage ? (
        <p
          className="rounded-2xl border border-primary/20 bg-primary/10 px-4 py-3 text-sm text-primary"
          data-testid="persona-catalog-feedback-notice"
        >
          {feedbackNoticeMessage}
        </p>
      ) : null}

      {feedbackErrorMessage ? (
        <p
          className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive"
          data-testid="persona-catalog-feedback-error"
        >
          {feedbackErrorMessage}
        </p>
      ) : null}
    </section>
  );
}
