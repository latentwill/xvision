// Static JSON fixture imports — Vite + resolveJsonModule bundle these at
// compile time. Do NOT convert to dynamic imports; the switch-on-literal
// pattern below is intentional so bundler tree-shaking works correctly.

import runFixture from "../__fixtures__/run.json";
import compareFixture from "../__fixtures__/compare.json";
import scenarioFixture from "../__fixtures__/scenario.json";
import strategyFixture from "../__fixtures__/strategy.json";
import liveFixture from "../__fixtures__/live.json";
import wizardFixture from "../__fixtures__/wizard.json";

import type {
  AnyChartV2Payload,
  CompareChartV2Payload,
  FixtureKey,
  LiveChartV2Payload,
  RunChartV2Payload,
  ScenarioChartV2Payload,
  StrategyChartV2Payload,
  WizardPreviewV2Payload,
} from "../types";

// Map from FixtureKey to the corresponding payload type, so callers get
// a precise return type when they pass a literal key.
type FixturePayloadMap = {
  run: RunChartV2Payload;
  compare: CompareChartV2Payload;
  scenario: ScenarioChartV2Payload;
  strategy: StrategyChartV2Payload;
  live: LiveChartV2Payload;
  wizard: WizardPreviewV2Payload;
};

/**
 * Non-React helper — returns the typed fixture payload for `key`.
 * Used by tests, Storybook stories, and the /chart-lab page.
 */
export function getChart2Fixture<K extends FixtureKey>(
  key: K,
): FixturePayloadMap[K] {
  switch (key) {
    case "run":
      return runFixture as unknown as FixturePayloadMap[K];
    case "compare":
      return compareFixture as unknown as FixturePayloadMap[K];
    case "scenario":
      return scenarioFixture as unknown as FixturePayloadMap[K];
    case "strategy":
      return strategyFixture as unknown as FixturePayloadMap[K];
    case "live":
      return liveFixture as unknown as FixturePayloadMap[K];
    case "wizard":
      return wizardFixture as unknown as FixturePayloadMap[K];
    default: {
      // Exhaustiveness guard — TypeScript narrows `key` to `never` here.
      const _exhaustive: never = key;
      throw new Error(`Unknown fixture key: ${String(_exhaustive)}`);
    }
  }
}

/**
 * React hook — returns the typed fixture payload for `key`.
 * The return value is a module-scope constant, so it is referentially
 * stable across re-renders (no useMemo needed).
 */
export function useChart2Fixture<K extends FixtureKey>(
  key: K,
): FixturePayloadMap[K] {
  return getChart2Fixture(key);
}

// Re-export the union type so consumers can import it from this module.
export type { AnyChartV2Payload, FixtureKey };
