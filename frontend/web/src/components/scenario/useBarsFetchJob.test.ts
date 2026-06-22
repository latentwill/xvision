import { describe, expect, it } from "vitest";
import { scenarioGranularityToCli } from "./useBarsFetchJob";

// CHART_GRANULARITY_OPTIONS from scenarios-detail.tsx — the set of values the
// menu can display. Any value NOT in this set leaves the picker with no matching
// option and no selected label (bead xvision-o24j).
const CHART_GRANULARITY_OPTION_VALUES = new Set([
  "1m",
  "5m",
  "15m",
  "1h",
  "4h",
  "6h",
  "1d",
  "1w",
]);

describe("scenarioGranularityToCli — blank-select guard (bead xvision-o24j)", () => {
  // Legacy backend enum strings that the switch maps explicitly. These were
  // the ONLY values covered before the fix.
  it.each([
    ["Hour1", "1h"],
    ["Hour4", "4h"],
    ["Hour6", "6h"],
    ["Day1", "1d"],
  ])(
    "maps legacy backend enum %s → %s (already worked)",
    (input, expected) => {
      expect(scenarioGranularityToCli(input)).toBe(expected);
    },
  );

  // Canonical CLI values (backend Serialize now emits these directly). They
  // must pass through unchanged and must be in the option set.
  it.each(["1m", "5m", "15m", "1h", "4h", "6h", "1d", "1w"])(
    "passes canonical CLI value %s through unchanged",
    (value) => {
      expect(scenarioGranularityToCli(value)).toBe(value);
    },
  );

  // Legacy backend enum strings that the switch did NOT cover before the fix.
  // These are the root cause of the blank-picker bug: without mapping them,
  // they pass through as e.g. "Minute1" which has no matching option.
  it.each([
    ["Minute1", "1m"],
    ["Minute5", "5m"],
    ["Minute15", "15m"],
    ["Week1", "1w"],
  ])(
    "maps missing legacy backend enum %s → %s (was the blank-select root cause)",
    (input, expected) => {
      expect(scenarioGranularityToCli(input)).toBe(expected);
    },
  );

  // Any value returned by scenarioGranularityToCli must be in the option set
  // so the menu always has a matching option and never renders blank.
  it.each(["Hour1", "Hour4", "Hour6", "Day1", "Minute1", "Minute5", "Minute15", "Week1", "1m", "5m", "15m", "1h", "4h", "6h", "1d", "1w"])(
    "output for %s is always in CHART_GRANULARITY_OPTIONS",
    (input) => {
      const result = scenarioGranularityToCli(input);
      expect(CHART_GRANULARITY_OPTION_VALUES.has(result)).toBe(true);
    },
  );
});
