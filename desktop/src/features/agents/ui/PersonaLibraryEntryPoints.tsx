import { Upload } from "lucide-react";

import { Button } from "@/shared/ui/button";

import { CreateNewButton } from "./CreateNewButton";
import { personaLibraryCopy } from "./personaLibraryCopy";

type PersonaLibraryEntryPointsProps = {
  canChooseCatalog?: boolean;
  isPending: boolean;
  layout: "header" | "empty";
  onCreate: () => void;
  onChooseCatalog?: () => void;
  onImport?: () => void;
};

export function PersonaLibraryEntryPoints({
  canChooseCatalog = false,
  isPending,
  layout,
  onCreate,
  onChooseCatalog,
  onImport,
}: PersonaLibraryEntryPointsProps) {
  const isHeader = layout === "header";
  const chooseVariant = isHeader ? "outline" : "default";
  const createVariant = isHeader ? "default" : "outline";

  return (
    <>
      {canChooseCatalog && onChooseCatalog ? (
        <Button
          data-testid="open-persona-catalog"
          disabled={isPending}
          onClick={onChooseCatalog}
          size="sm"
          type="button"
          variant={chooseVariant}
        >
          {personaLibraryCopy.chooseFromCatalog}
        </Button>
      ) : null}
      <CreateNewButton
        disabled={isPending}
        label={personaLibraryCopy.createNew}
        onClick={onCreate}
        variant={createVariant}
      />
      {!isHeader && onImport ? (
        <Button onClick={onImport} size="sm" type="button" variant="outline">
          <Upload className="h-4 w-4" />
          {personaLibraryCopy.import}
        </Button>
      ) : null}
    </>
  );
}
