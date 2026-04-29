// Registration entry point for the @/ path-alias resolver.
// Usage: node --import ./scripts/test-loader.mjs --test src/path/to/file.test.mjs
import { register } from "node:module";

register("./test-resolve-hook.mjs", import.meta.url);
