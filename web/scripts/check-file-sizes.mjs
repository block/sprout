import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, "..");

const rules = [
  {
    root: "src/app",
    extensions: new Set([".ts", ".tsx"]),
    maxLines: 500,
  },
  {
    root: "src/features",
    extensions: new Set([".ts", ".tsx"]),
    maxLines: 500,
  },
  {
    root: "src/shared/api",
    extensions: new Set([".ts", ".tsx"]),
    maxLines: 500,
  },
];

const overrides = new Map([]);

async function walkFiles(directory) {
  const entries = await fs.readdir(directory, { withFileTypes: true });
  const files = await Promise.all(
    entries.map(async (entry) => {
      const fullPath = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        return walkFiles(fullPath);
      }

      return [fullPath];
    }),
  );

  return files.flat();
}

function findRule(relativePath) {
  return rules.find((rule) => {
    const normalizedRoot = `${rule.root}${path.sep}`;
    return relativePath.startsWith(normalizedRoot);
  });
}

function countLines(content) {
  if (content.length === 0) {
    return 0;
  }

  return content.split(/\r?\n/).length;
}

const candidateFiles = (
  await Promise.all(
    rules.map((rule) => {
      const dir = path.join(projectRoot, rule.root);
      return fs
        .access(dir)
        .then(() => walkFiles(dir))
        .catch(() => []);
    }),
  )
).flat();

const violations = [];

for (const filePath of candidateFiles) {
  const relativePath = path.relative(projectRoot, filePath);
  const rule = findRule(relativePath);
  if (!rule) {
    continue;
  }

  const extension = path.extname(relativePath);
  if (!rule.extensions.has(extension)) {
    continue;
  }

  const limit = overrides.get(relativePath) ?? rule.maxLines;
  const content = await fs.readFile(filePath, "utf8");
  const lineCount = countLines(content);
  if (lineCount > limit) {
    violations.push({
      limit,
      lineCount,
      relativePath,
    });
  }
}

if (violations.length > 0) {
  console.error("Web file size check failed:");
  for (const violation of violations) {
    console.error(
      `- ${violation.relativePath}: ${violation.lineCount} lines (limit ${violation.limit})`,
    );
  }
  console.error(
    "Split the file or add a narrowly scoped exception in `web/scripts/check-file-sizes.mjs`.",
  );
  process.exit(1);
}
