import * as React from "react";

import { isCatalogPersonaSelected } from "@/features/agents/lib/catalog";
import type { AgentPersona } from "@/shared/api/types";

import { PersonaCatalogDetailsSheet } from "./PersonaCatalogDetailsSheet";
import { PersonaCatalogSection } from "./PersonaCatalogSection";

type PersonaCatalogSurfaceProps = {
  error: Error | null;
  feedbackErrorMessage: string | null;
  feedbackNoticeMessage: string | null;
  isLoading: boolean;
  isPending: boolean;
  onClearFeedback: () => void;
  onSelectPersona: (persona: AgentPersona, active: boolean) => void;
  personas: AgentPersona[];
  showHeader?: boolean;
};

export function PersonaCatalogSurface({
  error,
  feedbackErrorMessage,
  feedbackNoticeMessage,
  isLoading,
  isPending,
  onClearFeedback,
  onSelectPersona,
  personas,
  showHeader = true,
}: PersonaCatalogSurfaceProps) {
  const [detailPersonaId, setDetailPersonaId] = React.useState<string | null>(
    null,
  );
  const detailPersona = React.useMemo(
    () =>
      detailPersonaId
        ? (personas.find((persona) => persona.id === detailPersonaId) ?? null)
        : null,
    [detailPersonaId, personas],
  );
  const handleTogglePersona = (persona: AgentPersona) => {
    onSelectPersona(persona, !isCatalogPersonaSelected(persona));
  };

  return (
    <>
      <PersonaCatalogSection
        error={error}
        feedbackErrorMessage={feedbackErrorMessage}
        feedbackNoticeMessage={feedbackNoticeMessage}
        isLoading={isLoading}
        isPending={isPending}
        onTogglePersona={handleTogglePersona}
        onViewDetails={(persona) => {
          onClearFeedback();
          setDetailPersonaId(persona.id);
        }}
        personas={personas}
        showHeader={showHeader}
      />

      <PersonaCatalogDetailsSheet
        feedbackErrorMessage={feedbackErrorMessage}
        feedbackNoticeMessage={feedbackNoticeMessage}
        isPending={isPending}
        onOpenChange={(open) => {
          if (!open) {
            setDetailPersonaId(null);
          }
        }}
        onTogglePersona={handleTogglePersona}
        open={detailPersonaId !== null}
        persona={detailPersona}
      />
    </>
  );
}
