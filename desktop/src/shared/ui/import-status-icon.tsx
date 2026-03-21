import { Check, Loader2, X } from "lucide-react";

export type ImportItemStatus = "pending" | "importing" | "done" | "error";

/**
 * Tiny status indicator used in sequential-import dialogs (persona batch
 * import, team import). Renders a spinner while importing, a check on
 * success, an X on failure, and nothing while pending.
 */
export function ImportStatusIcon({
  status,
}: {
  status: ImportItemStatus | undefined;
}) {
  switch (status) {
    case "importing":
      return (
        <Loader2 className="h-4 w-4 shrink-0 animate-spin text-muted-foreground" />
      );
    case "done":
      return <Check className="h-4 w-4 shrink-0 text-green-500" />;
    case "error":
      return <X className="h-4 w-4 shrink-0 text-destructive" />;
    default:
      return null;
  }
}
