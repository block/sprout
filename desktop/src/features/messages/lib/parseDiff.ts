import {
  isDelete,
  isInsert,
  parseDiff,
  type DiffType,
  type FileData,
} from "react-diff-view";

type ParsedDiffResult = {
  files: FileData[];
  parseError: boolean;
};

function isRenderableFile(file: FileData) {
  return file.hunks.length > 0 || Boolean(file.oldPath || file.newPath);
}

export function parseUnifiedDiff(content: string): ParsedDiffResult {
  if (!content.trim()) {
    return { files: [], parseError: false };
  }

  try {
    const files = parseDiff(content).filter(isRenderableFile);

    if (!files.length) {
      return { files: [], parseError: true };
    }

    return { files, parseError: false };
  } catch {
    return { files: [], parseError: true };
  }
}

export function getDiffFileLabel(
  file: FileData,
  fallbackFilePath?: string,
): string {
  if (file.oldPath && file.newPath && file.oldPath !== file.newPath) {
    return `${file.oldPath} -> ${file.newPath}`;
  }

  return file.newPath || file.oldPath || fallbackFilePath || "diff";
}

export function countDiffFileChanges(file: FileData) {
  let additions = 0;
  let deletions = 0;

  for (const hunk of file.hunks) {
    for (const change of hunk.changes) {
      if (isInsert(change)) {
        additions += 1;
      } else if (isDelete(change)) {
        deletions += 1;
      }
    }
  }

  return { additions, deletions };
}

export function normalizeDiffType(type: string | undefined): DiffType {
  switch (type) {
    case "add":
    case "copy":
    case "delete":
    case "rename":
      return type;
    default:
      return "modify";
  }
}
