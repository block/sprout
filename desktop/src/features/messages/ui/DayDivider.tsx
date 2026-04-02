import { Separator } from "@/shared/ui/separator";

export function DayDivider({ label }: { label: string }) {
  return (
    <div
      className="flex items-center gap-3 py-2"
      data-testid="message-timeline-day-divider"
      data-day-label={label}
    >
      <Separator className="flex-1" />
      <p className="text-xs font-semibold uppercase tracking-[0.22em] text-muted-foreground">
        {label}
      </p>
      <Separator className="flex-1" />
    </div>
  );
}
