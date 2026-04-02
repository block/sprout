import { formatFullDateTime } from "@/features/messages/lib/dateFormatters";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";

export function MessageTimestamp({
  createdAt,
  time,
}: {
  createdAt: number;
  time: string;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <p className="cursor-default whitespace-nowrap">{time}</p>
      </TooltipTrigger>
      <TooltipContent side="top">
        {formatFullDateTime(createdAt)}
      </TooltipContent>
    </Tooltip>
  );
}
