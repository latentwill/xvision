import { afterEach, describe, expect, test, vi } from "vitest";

import {
  demoteAutoOptimizerRun,
  gateAutoOptimizerRun,
  gateOptimization,
  getAutoOptimizerRun,
  getFlywheelLineage,
  getFlywheelStatus,
  getFlywheelVelocity,
  listAutoOptimizerRuns,
  optimizeMemoryDemos,
  promoteAutoOptimizerRun,
  runAutoOptimizer,
} from "./flywheel";

function mockJson(body: unknown) {
  return Promise.resolve({
    ok: true,
    status: 200,
    json: () => Promise.resolve(body),
  } as Response);
}

describe("flywheel API", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  test("getFlywheelStatus builds the agent query URL", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() =>
        mockJson({
          namespace: "agent:A",
          observations: 2,
          active_patterns: 1,
          staged_patterns: 0,
          forgotten_patterns: 0,
          autooptimizer_runs: 1,
        }),
      );

    const status = await getFlywheelStatus({ agent: "A" });

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/flywheel/status?agent=A",
      expect.objectContaining({ headers: expect.any(Object) }),
    );
    expect(status.namespace).toBe("agent:A");
    expect(status.observations).toBe(2);
  });

  test("getFlywheelVelocity builds the lookback query URL", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() =>
        mockJson({
          namespace: "agent:A",
          days: 14,
          since: "2026-05-11T00:00:00Z",
          observations_captured: 3,
          patterns_promoted: 2,
          patterns_demoted: 1,
          autooptimizer_runs: 2,
          optimized_child_agents: 1,
          average_lineage_depth: 1,
          latest_activity_at: "2026-05-24T00:00:00Z",
        }),
      );

    const velocity = await getFlywheelVelocity({ agent: "A", days: 14 });

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/flywheel/velocity?agent=A&days=14",
      expect.objectContaining({ headers: expect.any(Object) }),
    );
    expect(velocity.namespace).toBe("agent:A");
    expect(velocity.optimized_child_agents).toBe(1);
  });

  test("getFlywheelLineage builds the lineage query URL", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() =>
        mockJson({
          namespace: "agent:A",
          total: 1,
          items: [
            {
              optimization_id: "opt-1",
              target_agent_id: "A",
              child_agent_id: "B",
              slot: "main",
              method: "memory-demos",
              demo_source: "frozen-snapshot",
              reproducible: true,
              holdout_split: "70/15/15",
              cohort_query: "namespace=agent:A,limit=8",
              train_observation_count: 1,
              dev_observation_count: 1,
              holdout_observation_count: 1,
              train_hash: "sha256:train",
              dev_hash: "sha256:dev",
              holdout_hash: "sha256:holdout",
              demo_source_pattern_ids: ["pat-demo"],
              prior_pattern_ids: ["pat-prior"],
              prompt_prefix_chars: 120,
              status: "minted",
              created_at: "2026-05-24T00:00:00Z",
              gate_verdict: "passed",
              delta_dev: 0.2,
              delta_holdout: 0.3,
              gate_reason: "child beat parent",
            },
          ],
        }),
      );

    const lineage = await getFlywheelLineage({ agent: "A", limit: 5 });

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/flywheel/lineage?agent=A&limit=5",
      expect.objectContaining({ headers: expect.any(Object) }),
    );
    expect(lineage.total).toBe(1);
    expect(lineage.items[0].prior_pattern_ids).toEqual(["pat-prior"]);
    expect(lineage.items[0].holdout_hash).toBe("sha256:holdout");
    expect(lineage.items[0].gate_verdict).toBe("passed");
    expect(lineage.items[0].delta_holdout).toBe(0.3);
  });

  test("runAutoOptimizer posts the provided embedding body", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() =>
        mockJson({
          id: "ar_1",
          namespace: "agent:A",
          observation_ids: ["obs_1", "obs_2"],
          pattern_id: "pat_1",
          pattern_text: "reduce risk",
          promotion_state: "staged",
          min_observations: 2,
          created_at: "2026-05-25T00:00:00Z",
          status: "completed",
        }),
      );

    const run = await runAutoOptimizer({
      agent: "A",
      pattern_text: "reduce risk",
      embedding: [1, 0],
      min_observations: 2,
    });

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/autooptimizer/run",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          agent: "A",
          pattern_text: "reduce risk",
          embedding: [1, 0],
          min_observations: 2,
        }),
      }),
    );
    expect(run.pattern_id).toBe("pat_1");
  });

  test("getAutoOptimizerRun encodes ids", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() =>
        mockJson({
          id: "ar/1",
          namespace: "global",
          observation_ids: [],
          pattern_id: "pat",
          pattern_text: "p",
          promotion_state: "staged",
          min_observations: 2,
          created_at: "2026-05-25T00:00:00Z",
          status: "completed",
        }),
      );

    await getAutoOptimizerRun("ar/1");

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/autooptimizer/ar%2F1",
      expect.any(Object),
    );
  });

  test("listAutoOptimizerRuns builds scoped list URLs", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() =>
        mockJson({
          items: [],
          total: 0,
        }),
      );

    await listAutoOptimizerRuns({ agent: "A", limit: 10, offset: 5 });

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/autooptimizer?agent=A&limit=10&offset=5",
      expect.any(Object),
    );
  });

  test("promote and demote autooptimizer runs encode ids", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() =>
        mockJson({
          id: "ar/1",
          namespace: "global",
          observation_ids: [],
          pattern_id: "pat",
          pattern_text: "p",
          promotion_state: "active",
          min_observations: 2,
          created_at: "2026-05-25T00:00:00Z",
          status: "completed",
        }),
      );

    await promoteAutoOptimizerRun("ar/1");
    await demoteAutoOptimizerRun("ar/1");

    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "/api/autooptimizer/ar%2F1/promote",
      expect.objectContaining({ method: "POST" }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "/api/autooptimizer/ar%2F1/demote",
      expect.objectContaining({ method: "POST" }),
    );
  });

  test("gateAutoOptimizerRun posts numeric gate and finding body", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() =>
        mockJson({
          id: "ar/1",
          namespace: "global",
          observation_ids: [],
          pattern_id: "pat",
          pattern_text: "p",
          promotion_state: "staged",
          min_observations: 2,
          created_at: "2026-05-25T00:00:00Z",
          status: "completed",
          gate_passed: true,
          gate_verdict: "passed",
          delta_day: 0.2,
          delta_holdout: 0.2,
          finding_blind: true,
          finding_blinded_metrics: true,
        }),
      );

    await gateAutoOptimizerRun("ar/1", {
      metric: "sharpe_delta",
      parent_day_score: 0.7,
      child_day_score: 0.9,
      parent_holdout_score: 1,
      child_holdout_score: 1.2,
      gate_epsilon: 0.1,
      finding_text: "Blind Finding",
      qualitative_finding_json: "{\"summary\":\"Blind Finding\"}",
      finding_blinded_metrics: true,
      judge_model: "test-judge",
    });

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/autooptimizer/ar%2F1/gate",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          metric: "sharpe_delta",
          parent_day_score: 0.7,
          child_day_score: 0.9,
          parent_holdout_score: 1,
          child_holdout_score: 1.2,
          gate_epsilon: 0.1,
          finding_text: "Blind Finding",
          qualitative_finding_json: "{\"summary\":\"Blind Finding\"}",
          finding_blinded_metrics: true,
          judge_model: "test-judge",
        }),
      }),
    );
  });

  test("optimizeMemoryDemos posts child mint requests", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() =>
        mockJson({
          optimization_id: "opt-1",
          status: "minted",
          namespace: "agent:A",
          target_agent_id: "A",
          child_agent_id: "B",
          slot: "main",
          demo_count: 2,
          demo_source: "frozen-snapshot",
          reproducible: true,
          holdout_split: "70/15/15",
          cohort_query: "namespace=agent:A,limit=8",
          observation_ids: ["obs_1", "obs_2"],
          train_observation_ids: ["obs_1", "obs_2"],
          dev_observation_ids: ["obs_3"],
          holdout_observation_ids: ["obs_4"],
          train_hash: "sha256:train",
          dev_hash: "sha256:dev",
          holdout_hash: "sha256:holdout",
          demo_source_pattern_ids: ["pat-demo-1"],
          pattern_demo_source_count: 1,
          prior_pattern_ids: ["pat-1"],
          pattern_prior_count: 1,
          observations: [],
          prompt_prefix_chars: 120,
          prompt_preview: "<memory_demos />",
        }),
      );

    const result = await optimizeMemoryDemos({
      target_agent_id: "A",
      demo_source: "frozen-snapshot",
      holdout_split: "70/15/15",
      cohort_query: "regime=Trend",
      prior_pattern_ids: ["pat-1"],
      apply: true,
      child_name: "A child",
    });

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/optimize/memory-demos",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          target_agent_id: "A",
          demo_source: "frozen-snapshot",
          holdout_split: "70/15/15",
          cohort_query: "regime=Trend",
          prior_pattern_ids: ["pat-1"],
          apply: true,
          child_name: "A child",
        }),
      }),
    );
    expect(result.child_agent_id).toBe("B");
    expect(result.optimization_id).toBe("opt-1");
    expect(result.holdout_hash).toBe("sha256:holdout");
    expect(result.demo_source_pattern_ids).toEqual(["pat-demo-1"]);
    expect(result.pattern_demo_source_count).toBe(1);
    expect(result.prior_pattern_ids).toEqual(["pat-1"]);
    expect(result.pattern_prior_count).toBe(1);
  });

  test("gateOptimization posts optimizer holdout gate scores", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() =>
        mockJson({
          optimization_id: "opt/1",
          dev_metric: "sharpe_delta",
          holdout_metric: "sharpe_delta",
          parent_dev_score: 0.7,
          child_dev_score: 0.9,
          parent_holdout_score: 1,
          child_holdout_score: 1.2,
          gate_epsilon: 0.1,
          delta_dev: 0.2,
          delta_holdout: 0.2,
          gate_verdict: "passed",
          gate_reason: "child beat parent",
          gated_at: "2026-05-25T00:00:00Z",
        }),
      );

    const result = await gateOptimization("opt/1", {
      dev_metric: "sharpe_delta",
      parent_dev_score: 0.7,
      child_dev_score: 0.9,
      parent_holdout_score: 1,
      child_holdout_score: 1.2,
      gate_epsilon: 0.1,
      gate_reason: "child beat parent",
    });

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/optimize/memory-demos/opt%2F1/gate",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          dev_metric: "sharpe_delta",
          parent_dev_score: 0.7,
          child_dev_score: 0.9,
          parent_holdout_score: 1,
          child_holdout_score: 1.2,
          gate_epsilon: 0.1,
          gate_reason: "child beat parent",
        }),
      }),
    );
    expect(result.gate_verdict).toBe("passed");
    expect(result.delta_holdout).toBe(0.2);
  });
});
