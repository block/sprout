/** PNG file signature (first 4 bytes). */
export const PNG_MAGIC = [0x89, 0x50, 0x4e, 0x47] as const;

/** ZIP local-file-header signature (first 4 bytes). */
export const ZIP_MAGIC = [0x50, 0x4b, 0x03, 0x04] as const;

/** Opening brace `{` — first byte of a JSON file. */
export const JSON_FIRST_BYTE = 0x7b;

/** Check whether `bytes` starts with the given magic sequence. */
export function matchesMagic(
  bytes: number[] | readonly number[],
  magic: readonly number[],
): boolean {
  return magic.every((b, i) => bytes[i] === b);
}

/** Return true when `bytes` looks like a single-item file (PNG or JSON). */
export function isSingleItemFile(bytes: number[] | readonly number[]): boolean {
  return (
    matchesMagic(bytes, PNG_MAGIC) ||
    (bytes.length > 0 && bytes[0] === JSON_FIRST_BYTE)
  );
}
