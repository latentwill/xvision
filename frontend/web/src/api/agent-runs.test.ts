// frontend/web/src/api/agent-runs.test.ts
import { describe, expect, test } from "vitest";
import { getAgentRun, agentRunKeys } from "./agent-runs";

describe("agent-runs API (mock mode)", () => {
  test("getAgentRun returns the canned completed run", async () => {
    const detail = await getAgentRun("run_abc1234");
    expect(detail.summary.run_id).toBe("run_abc1234");
    expect(detail.spans.length).toBeGreaterThan(0);
  });

  test("getAgentRun for unknown id rejects with not_found", async () => {
    await expect(getAgentRun("missing")).rejects.toMatchObject({
      code: "not_found",
    });
  });

  test("agentRunKeys.run produces a stable cache key", () => {
    expect(agentRunKeys.run("x")).toEqual(["agent-runs", "run", "x"]);
  });
});
