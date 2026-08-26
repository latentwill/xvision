import { describe, expect, test } from "vitest";

import { displayScenarioName, evalRunLabels } from "./run-display";
import type { RunSummary } from "@/api/types.gen";

describe("forward-test run display", () => {
  test("treats fwd mode as the forward-test scenario", () => {
    expect(
      displayScenarioName("", [], "fwd", {
        bar_limit: 12,
        decision_limit: null,
        time_limit_secs: undefined,
        trade_limit: null,
      }),
    ).toBe("Forward Test · 12 bars");
  });

  test("formats fwd mode subtitle without legacy live copy", () => {
    const summary = {
      id: "run_fwd",
      agent_id: "strategy_1",
      scenario_id: "",
      mode: "fwd",
      status: "running",
      started_at: null,
      finished_at: null,
      strategy: { display_name: "Momentum" },
      scenario: null,
      live_config: null,
    } as unknown as RunSummary;

    expect(evalRunLabels(summary).subtitle).toBe("fwd · running");
  });
});
