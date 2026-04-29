export function getToolString(
  record: Record<string, unknown>,
  keys: string[],
): string | null {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "string" && value.trim().length > 0) {
      return value.trim();
    }
  }
  return null;
}

export function getToolStringList(
  record: Record<string, unknown>,
  keys: string[],
): string[] {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "string" && value.trim().length > 0) {
      return [value.trim()];
    }
    if (Array.isArray(value)) {
      return value.filter(
        (item): item is string =>
          typeof item === "string" && item.trim().length > 0,
      );
    }
  }
  return [];
}

export function getResultArray(
  resultValue: unknown,
  resultRecord: Record<string, unknown>,
  key: string,
) {
  if (Array.isArray(resultValue)) return resultValue;
  const value = resultRecord[key];
  return Array.isArray(value) ? value : null;
}

export function formatCodeValue(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) return value;
  try {
    return JSON.stringify(JSON.parse(trimmed), null, 2);
  } catch {
    return value;
  }
}

export function titleCase(value: string): string {
  return value
    .replace(/[_-]+/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/\b\w/g, (match) => match.toUpperCase());
}

export function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object"
    ? (value as Record<string, unknown>)
    : {};
}

export function asString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

export function shorten(value: string) {
  return value.length > 14
    ? `${value.slice(0, 8)}...${value.slice(-4)}`
    : value;
}

export function shortenMiddle(value: string, maxLength: number) {
  if (value.length <= maxLength) return value;
  const edgeLength = Math.max(4, Math.floor((maxLength - 3) / 2));
  return `${value.slice(0, edgeLength)}...${value.slice(-edgeLength)}`;
}
