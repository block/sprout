import { Check, Copy } from "lucide-react";
import * as React from "react";

import { Button } from "@/shared/ui/button";

export function CopyButton({
  value,
  label,
}: {
  value: string;
  label?: string;
}) {
  const [copied, setCopied] = React.useState(false);

  return (
    <Button
      onClick={async () => {
        await navigator.clipboard.writeText(value);
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1_500);
      }}
      size="sm"
      type="button"
      variant="outline"
    >
      {copied ? (
        <Check className="h-3.5 w-3.5" />
      ) : (
        <Copy className="h-3.5 w-3.5" />
      )}
      <span>{copied ? "Copied" : (label ?? "Copy")}</span>
    </Button>
  );
}
