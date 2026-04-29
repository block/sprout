/**
 * ESM resolve hook that maps `@/*` imports to `<projectRoot>/src/*`.
 * Also resolves extensionless imports to `.ts` files (TypeScript convention).
 */

import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";
import fs from "node:fs";

const projectRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const srcDir = path.join(projectRoot, "src");

/**
 * Try to resolve a file path that may be missing its extension.
 * Returns the resolved file URL or null.
 */
function tryResolveFile(filePath) {
  // Try exact path first
  if (fs.existsSync(filePath) && fs.statSync(filePath).isFile()) {
    return pathToFileURL(filePath).href;
  }
  // Try with .ts extension
  const withTs = `${filePath}.ts`;
  if (fs.existsSync(withTs)) {
    return pathToFileURL(withTs).href;
  }
  // Try with .tsx extension
  const withTsx = `${filePath}.tsx`;
  if (fs.existsSync(withTsx)) {
    return pathToFileURL(withTsx).href;
  }
  // Try as directory with index.ts
  const indexTs = path.join(filePath, "index.ts");
  if (fs.existsSync(indexTs)) {
    return pathToFileURL(indexTs).href;
  }
  return null;
}

export function resolve(specifier, context, nextResolve) {
  if (specifier.startsWith("@/")) {
    const bare = path.join(srcDir, specifier.slice(2));
    const resolved = tryResolveFile(bare);
    if (resolved) {
      return nextResolve(resolved, context);
    }
    // Fallback — let Node try (will likely fail with a clear error)
    return nextResolve(pathToFileURL(bare).href, context);
  }
  return nextResolve(specifier, context);
}
