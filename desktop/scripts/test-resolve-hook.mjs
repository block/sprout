// Node.js customization hooks — resolves @/ path alias and .ts extensions.
// Loaded via: node --import ./scripts/test-loader.mjs
import { existsSync } from "node:fs";
import { resolve as pathResolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const srcDir = pathResolve(fileURLToPath(import.meta.url), "../../src");

export function resolve(specifier, context, nextResolve) {
  let mapped = specifier;

  // Resolve @/ alias to src/
  if (mapped.startsWith("@/")) {
    mapped = pathToFileURL(pathResolve(srcDir, mapped.slice(2))).href;
  }

  // If the specifier (after alias resolution) looks like a local/file path
  // without an extension, try appending .ts
  if (
    mapped.startsWith("file://") &&
    !mapped.endsWith(".ts") &&
    !mapped.endsWith(".mjs") &&
    !mapped.endsWith(".js")
  ) {
    const filePath = fileURLToPath(mapped);
    if (!existsSync(filePath) && existsSync(filePath + ".ts")) {
      mapped = mapped + ".ts";
    }
  }

  if (mapped !== specifier) {
    return nextResolve(mapped, context);
  }
  return nextResolve(specifier, context);
}
