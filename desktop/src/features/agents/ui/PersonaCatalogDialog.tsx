import * as React from "react";

import type { AgentPersona } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import { Dialog } from "@/shared/ui/dialog";
import { ChooserDialogContent } from "@/shared/ui/chooser-dialog-content";

import { PersonaCatalogSurface } from "./PersonaCatalogSurface";
import { personaCatalogCopy } from "./personaLibraryCopy";

type PersonaCatalogDialogProps = {
  error: Error | null;
  feedbackErrorMessage: string | null;
  feedbackNoticeMessage: string | null;
  isLoading: boolean;
  isPending: boolean;
  onClearFeedback: () => void;
  onOpenChange: (open: boolean) => void;
  onSelectPersona: (persona: AgentPersona, active: boolean) => void;
  open: boolean;
  personas: AgentPersona[];
};

export function PersonaCatalogDialog({
  error,
  feedbackErrorMessage,
  feedbackNoticeMessage,
  isLoading,
  isPending,
  onClearFeedback,
  onOpenChange,
  onSelectPersona,
  open,
  personas,
}: PersonaCatalogDialogProps) {
  const contentRef = React.useRef<HTMLDivElement | null>(null);

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <ChooserDialogContent
        className="max-w-5xl"
        data-testid="persona-catalog-dialog"
        description={personaCatalogCopy.dialogDescription}
        footer={
          <Button
            data-testid="persona-catalog-dialog-done"
            onClick={() => onOpenChange(false)}
            size="sm"
            type="button"
            variant="outline"
          >
            Done
          </Button>
        }
        footerClassName="justify-end gap-2"
        footerTestId="persona-catalog-dialog-footer"
        headerTestId="persona-catalog-dialog-header"
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          contentRef.current?.focus();
        }}
        ref={contentRef}
        scrollAreaTestId="persona-catalog-dialog-scroll-area"
        tabIndex={-1}
        title={personaCatalogCopy.dialogTitle}
      >
        <PersonaCatalogSurface
          error={error}
          feedbackErrorMessage={feedbackErrorMessage}
          feedbackNoticeMessage={feedbackNoticeMessage}
          isLoading={isLoading}
          isPending={isPending}
          onClearFeedback={onClearFeedback}
          onSelectPersona={onSelectPersona}
          personas={personas}
          showHeader={false}
        />
      </ChooserDialogContent>
    </Dialog>
  );
}
