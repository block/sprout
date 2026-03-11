import { parse } from "diff2html";

export function parseDiffToOldNew(unifiedDiff: string): {
  original: string;
  modified: string;
} {
  try {
    const files = parse(unifiedDiff);
    if (!files.length) {
      // diff2html couldn't parse any files — show raw diff as fallback
      return { original: "", modified: unifiedDiff };
    }

    const originalLines: string[] = [];
    const modifiedLines: string[] = [];

    for (const file of files) {
      // Add file header separator for multi-file diffs
      if (files.length > 1) {
        const header = `// ── ${file.newName || file.oldName || "unknown"} ──`;
        originalLines.push(header);
        modifiedLines.push(header);
      }

      for (const block of file.blocks) {
        for (const line of block.lines) {
          if (line.content.startsWith("\\ ")) continue;
          if (line.type === "context" || line.type === "delete") {
            originalLines.push(line.content.slice(1));
          }
          if (line.type === "context" || line.type === "insert") {
            modifiedLines.push(line.content.slice(1));
          }
        }
      }

      // Add blank line between files
      if (files.length > 1) {
        originalLines.push("");
        modifiedLines.push("");
      }
    }

    return {
      original: originalLines.join("\n"),
      modified: modifiedLines.join("\n"),
    };
  } catch {
    // Malformed diff — return raw content as fallback
    return { original: "", modified: unifiedDiff };
  }
}
