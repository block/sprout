import { isCatalogPersonaSelected } from "@/features/agents/lib/catalog";
import type { AgentPersona } from "@/shared/api/types";
import { promptPreview } from "@/shared/lib/promptPreview";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Card } from "@/shared/ui/card";
import { Skeleton } from "@/shared/ui/skeleton";
import { PersonaCatalogSelectionBadge } from "./PersonaCatalogSelectionBadge";
import { PersonaIdentity } from "./PersonaIdentity";
import {
  getPersonaCatalogSelectionActionCopy,
  getPersonaCatalogSelectionAriaLabel,
  personaCatalogCopy,
} from "./personaLibraryCopy";

type PersonaCatalogSectionProps = {
  emptyDescription?: string;
  emptyTitle?: string;
  error: Error | null;
  isLoading: boolean;
  isPending: boolean;
  onTogglePersona: (persona: AgentPersona) => void;
  onViewDetails: (persona: AgentPersona) => void;
  personas: AgentPersona[];
  showHeader?: boolean;
};

export function PersonaCatalogSection({
  emptyDescription = personaCatalogCopy.emptyCatalogDescription,
  emptyTitle = personaCatalogCopy.emptyCatalogTitle,
  error,
  isLoading,
  isPending,
  onTogglePersona,
  onViewDetails,
  personas,
  showHeader = true,
}: PersonaCatalogSectionProps) {
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
            <Card className="p-3" key={key}>
              <div className="flex items-center gap-2.5">
                <Skeleton className="h-8 w-8 rounded-lg" />
                <div className="space-y-2">
                  <Skeleton className="h-4 w-24" />
                  <Skeleton className="h-3 w-20" />
                </div>
              </div>
            </Card>
          ))}
        </div>
      ) : null}

      {!isLoading && personas.length > 0 ? (
        <div className="grid gap-3 md:grid-cols-3 xl:grid-cols-4">
          {personas.map((persona) => {
            const preview = promptPreview(persona.systemPrompt);
            const isSelected = isCatalogPersonaSelected(persona);

            return (
              <div
                className={cn(
                  "group relative flex flex-col gap-4 rounded-xl border p-3 shadow-sm transition-[background-color,border-color,box-shadow]",
                  isPending
                    ? "cursor-not-allowed opacity-70"
                    : "cursor-pointer",
                  isSelected
                    ? "border-primary bg-primary/10 text-foreground"
                    : "border-border/80 bg-background/60 hover:bg-accent hover:text-accent-foreground",
                )}
                data-testid={`persona-catalog-card-${persona.id}`}
                data-state={isSelected ? "selected" : "available"}
                key={persona.id}
              >
                <button
                  aria-label={getPersonaCatalogSelectionAriaLabel(
                    persona.displayName,
                    isSelected,
                  )}
                  aria-pressed={isSelected}
                  className="absolute inset-0 z-0 rounded-xl focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 focus-visible:ring-offset-2"
                  data-testid={`persona-catalog-card-target-${persona.id}`}
                  disabled={isPending}
                  onClick={() => {
                    onTogglePersona(persona);
                  }}
                  type="button"
                />

                <div className="pointer-events-none relative z-10 flex h-full flex-col gap-4">
                  <div className="flex items-start justify-between gap-3">
                    <PersonaIdentity
                      className="min-w-0 flex-1"
                      persona={persona}
                      showBuiltInBadge={false}
                      showPromptTooltip={false}
                    />

                    <PersonaCatalogSelectionBadge isActive={isSelected} />
                  </div>

                  <p className="min-h-12 text-xs leading-5 text-muted-foreground">
                    {preview}
                  </p>

                  <div
                    className={cn(
                      "mt-auto flex items-center justify-between gap-3 border-t pt-3",
                      isSelected ? "border-primary/20" : "border-border/60",
                    )}
                  >
                    <Button
                      className="pointer-events-auto"
                      data-testid={`persona-catalog-details-${persona.id}`}
                      onClick={() => {
                        onViewDetails(persona);
                      }}
                      size="sm"
                      type="button"
                      variant="ghost"
                    >
                      {personaCatalogCopy.detailsAction}
                    </Button>

                    <span className="text-xs font-medium text-muted-foreground">
                      {getPersonaCatalogSelectionActionCopy(isSelected)}
                    </span>
                  </div>
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
    </section>
  );
}
