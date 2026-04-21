import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";

type VerifiedBadgeProps = {
  verifiedName: string;
};

export function VerifiedBadge({ verifiedName }: VerifiedBadgeProps) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="ml-0.5 inline-flex items-center text-blue-500">
          <svg
            aria-label="Verified"
            className="h-3.5 w-3.5"
            fill="currentColor"
            role="img"
            viewBox="0 0 20 20"
            xmlns="http://www.w3.org/2000/svg"
          >
            <path
              clipRule="evenodd"
              d="M6.267 3.455a3.066 3.066 0 001.745-.723 3.066 3.066 0 013.976 0 3.066 3.066 0 001.745.723 3.066 3.066 0 012.812 2.812c.051.643.304 1.254.723 1.745a3.066 3.066 0 010 3.976 3.066 3.066 0 00-.723 1.745 3.066 3.066 0 01-2.812 2.812 3.066 3.066 0 00-1.745.723 3.066 3.066 0 01-3.976 0 3.066 3.066 0 00-1.745-.723 3.066 3.066 0 01-2.812-2.812 3.066 3.066 0 00-.723-1.745 3.066 3.066 0 010-3.976 3.066 3.066 0 00.723-1.745 3.066 3.066 0 012.812-2.812zm7.44 5.252a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z"
              fillRule="evenodd"
            />
          </svg>
        </span>
      </TooltipTrigger>
      <TooltipContent side="top">
        <p className="text-xs">Verified as {verifiedName}</p>
      </TooltipContent>
    </Tooltip>
  );
}
