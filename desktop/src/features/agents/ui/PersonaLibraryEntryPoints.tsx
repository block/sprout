import { Upload } from "lucide-react";

import { Button } from "@/shared/ui/button";

import { CreateNewButton } from "./CreateNewButton";
import { personaLibraryCopy } from "./personaLibraryCopy";

type PersonaLibraryEntryPointsProps = {
  isPending: boolean;
  layout: "header" | "empty";
  onCreate: () => void;
  onImport?: () => void;
};

export function PersonaLibraryEntryPoints({
  isPending,
  layout,
  onCreate,
  onImport,
}: PersonaLibraryEntryPointsProps) {
  const isHeader = layout === "header";

  return (
    <>
      <CreateNewButton
        disabled={isPending}
        label={personaLibraryCopy.createNew}
        onClick={onCreate}
        variant={isHeader ? "default" : "outline"}
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
