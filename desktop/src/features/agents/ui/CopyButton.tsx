import { Copy } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/shared/ui/button";

export function CopyButton({
  value,
  label,
}: {
  value: string;
  label?: string;
}) {
  return (
    <Button
      onClick={async () => {
        await navigator.clipboard.writeText(value);
        toast.success("Copied to clipboard");
      }}
      size="sm"
      type="button"
      variant="outline"
    >
      <Copy className="h-3.5 w-3.5" />
      <span>{label ?? "Copy"}</span>
    </Button>
  );
}
