import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { resolveEnabled } from "./resolveEnabled.ts";

describe("resolveEnabled", () => {
  describe("stable tier", () => {
    it("always returns true regardless of overrides or env", () => {
      assert.equal(resolveEnabled("stable", "channels", {}, false, false), true);
      assert.equal(resolveEnabled("stable", "channels", { channels: false }, false, true), true);
      assert.equal(resolveEnabled("stable", "channels", {}, true, true), true);
    });
  });

  describe("experimental tier", () => {
    it("returns false by default (no override)", () => {
      assert.equal(resolveEnabled("experimental", "workflows", {}, true, true), false);
    });

    it("returns true when user opts in", () => {
      assert.equal(
        resolveEnabled("experimental", "workflows", { workflows: true }, true, true),
        true,
      );
    });

    it("returns false when user explicitly opts out", () => {
      assert.equal(
        resolveEnabled("experimental", "workflows", { workflows: false }, true, true),
        false,
      );
    });

    it("ignores dev toggle and isDev", () => {
      assert.equal(
        resolveEnabled("experimental", "workflows", { workflows: true }, false, false),
        true,
      );
    });
  });

  describe("dev tier", () => {
    it("returns false in production builds even with devToggle on", () => {
      assert.equal(resolveEnabled("dev", "doctor", {}, true, false), false);
    });

    it("returns false in dev builds when devToggle is off", () => {
      assert.equal(resolveEnabled("dev", "doctor", {}, false, true), false);
    });

    it("returns true in dev builds with devToggle on and no override", () => {
      assert.equal(resolveEnabled("dev", "doctor", {}, true, true), true);
    });

    it("returns false when per-feature override is explicitly false", () => {
      assert.equal(
        resolveEnabled("dev", "doctor", { doctor: false }, true, true),
        false,
      );
    });

    it("returns true when per-feature override is explicitly true", () => {
      assert.equal(
        resolveEnabled("dev", "doctor", { doctor: true }, true, true),
        true,
      );
    });
  });

  describe("unknown tier", () => {
    it("returns false for unrecognized tier values", () => {
      // @ts-expect-error — testing invalid input
      assert.equal(resolveEnabled("unknown", "foo", {}, true, true), false);
    });
  });
});
