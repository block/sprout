import { Separator } from "@/shared/ui/separator";

export function DayDivider({ label }: { label: string }) {
  return (
    <section
      aria-label={label}
      className="sticky top-0 z-30 flex items-center gap-2.5 bg-background/80 py-1 backdrop-blur-sm"
      data-testid="message-timeline-day-divider"
      data-day-label={label}
    >
      <Separator className="flex-1 bg-border/35" />
      <p className="text-[10px] font-medium tracking-[0.02em] text-muted-foreground/65">
        {label}
      </p>
      <Separator className="flex-1 bg-border/35" />
    </section>
  );
}
