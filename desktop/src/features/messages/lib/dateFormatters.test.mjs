import assert from "node:assert/strict";
import test from "node:test";

import { formatShortMonthDayOrdinal } from "./dateFormatters.ts";

function localUnixSeconds(year, monthIndex, day) {
  return new Date(year, monthIndex, day, 12).getTime() / 1_000;
}

test("formatShortMonthDayOrdinal formats month before ordinal day", () => {
  assert.equal(
    formatShortMonthDayOrdinal(localUnixSeconds(2026, 4, 19)),
    "May 19th",
  );
});

test("formatShortMonthDayOrdinal handles ordinal suffixes", () => {
  assert.equal(
    formatShortMonthDayOrdinal(localUnixSeconds(2026, 4, 1)),
    "May 1st",
  );
  assert.equal(
    formatShortMonthDayOrdinal(localUnixSeconds(2026, 4, 2)),
    "May 2nd",
  );
  assert.equal(
    formatShortMonthDayOrdinal(localUnixSeconds(2026, 4, 3)),
    "May 3rd",
  );
  assert.equal(
    formatShortMonthDayOrdinal(localUnixSeconds(2026, 4, 4)),
    "May 4th",
  );
  assert.equal(
    formatShortMonthDayOrdinal(localUnixSeconds(2026, 4, 11)),
    "May 11th",
  );
  assert.equal(
    formatShortMonthDayOrdinal(localUnixSeconds(2026, 4, 12)),
    "May 12th",
  );
  assert.equal(
    formatShortMonthDayOrdinal(localUnixSeconds(2026, 4, 13)),
    "May 13th",
  );
  assert.equal(
    formatShortMonthDayOrdinal(localUnixSeconds(2026, 4, 21)),
    "May 21st",
  );
  assert.equal(
    formatShortMonthDayOrdinal(localUnixSeconds(2026, 4, 22)),
    "May 22nd",
  );
  assert.equal(
    formatShortMonthDayOrdinal(localUnixSeconds(2026, 4, 23)),
    "May 23rd",
  );
  assert.equal(
    formatShortMonthDayOrdinal(localUnixSeconds(2026, 4, 31)),
    "May 31st",
  );
});
