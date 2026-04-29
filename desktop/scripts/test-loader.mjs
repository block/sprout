/**
 * Custom Node ESM loader that resolves `@/` path aliases to `./src/`
 * relative to the desktop project root.
 *
 * Usage:
 *   node --experimental-strip-types --import ./scripts/test-loader.mjs --test src/path/to.test.mjs
 */

import { register } from "node:module";

register("./resolve-at-alias.mjs", import.meta.url);
