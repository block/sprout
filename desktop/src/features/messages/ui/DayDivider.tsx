import { Separator } from "@/shared/ui/separator";

export function DayDivider({ label }: { label: string }) {
  return (
    <section
      aria-label={label}
      className="flex items-center gap-2 py-1"
      data-testid="message-timeline-day-divider"
      data-day-label={label}
    >
      <Separator className="flex-1 bg-border/50" />
      <p className="shrink-0 text-[11px] font-medium leading-none tracking-normal text-muted-foreground/70">
        {label}
      </p>
      <Separator className="flex-1 bg-border/50" />
    </section>
  );
}
