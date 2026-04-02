import { formatFullDateTime } from "@/features/messages/lib/dateFormatters";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/shared/ui/tooltip";

export function MessageTimestamp({
  createdAt,
  time,
}: {
  createdAt: number;
  time: string;
}) {
  return (
    <TooltipProvider delayDuration={200}>
      <Tooltip>
        <TooltipTrigger asChild>
          <p className="cursor-default whitespace-nowrap">{time}</p>
        </TooltipTrigger>
        <TooltipContent side="top">
          {formatFullDateTime(createdAt)}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
