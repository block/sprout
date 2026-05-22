import type { AcpProvider } from "@/shared/api/types";

/**
 * Sort ACP providers with "goose" first, then alphabetically by label.
 * Used by any surface that presents a provider list to the user.
 */
export function sortProviders(
  providers: readonly AcpProvider[],
): AcpProvider[] {
  return [...providers].sort((left, right) => {
    const leftPriority = left.id === "goose" ? 0 : 1;
    const rightPriority = right.id === "goose" ? 0 : 1;
    if (leftPriority !== rightPriority) {
      return leftPriority - rightPriority;
    }
    return left.label.localeCompare(right.label);
  });
}
