import { afterEach, describe, expect, it, vi } from "vitest";

import * as client from "./client";
import {
  buildDeploymentsListUrl,
  deploymentKeys,
  listDeployments,
} from "./live-deployments";
import type { LiveDeploymentSummary } from "./types.gen";

function makeSummary(
  over: Partial<LiveDeploymentSummary> = {},
): LiveDeploymentSummary {
  return {
    deployment_id: "dep-1",
    strategy_id: "strat-1",
    strategy_name: "Alpha",
    mode: "paper",
    status: "running",
    started_at: "2026-06-13T00:00:00Z",
    last_decision_at: "2026-06-13T01:00:00Z",
    venue: "alpaca-paper",
    venue_connected: true,
    deployed_capital_usd: 1000,
    realized_pnl_usd: 0,
    unrealized_pnl_usd: 12.5,
    drawdown_pct: 1.2,
    daily_loss_limit_remaining_usd: 500,
    risk_veto_count_since_last_visit: null,
    paused: false,
    flatten_requested: false,
    global_safety_paused: false,
    source: "human",
    unavailable_reason: null,
    ...over,
  };
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("buildDeploymentsListUrl", () => {
  it("returns the bare path when no params are given", () => {
    expect(buildDeploymentsListUrl()).toBe("/api/live/deployments");
  });

  it("encodes the status filter as a query param", () => {
    expect(buildDeploymentsListUrl({ status: "running,paused" })).toBe(
      "/api/live/deployments?status=running%2Cpaused",
    );
  });

  it("omits empty status", () => {
    expect(buildDeploymentsListUrl({ status: "" })).toBe(
      "/api/live/deployments",
    );
  });

  it("includes mode and limit when present", () => {
    const url = buildDeploymentsListUrl({ mode: "live", limit: 5 });
    expect(url).toContain("mode=live");
    expect(url).toContain("limit=5");
  });
});

describe("deploymentKeys", () => {
  it("namespaces under 'live-deployments'", () => {
    expect(deploymentKeys.all).toEqual(["live-deployments"]);
  });

  it("list key folds the params into a stable tuple", () => {
    const a = deploymentKeys.list({ status: "running,paused" });
    const b = deploymentKeys.list({ status: "running,paused" });
    expect(a).toEqual(b);
    expect(a).toContain("running,paused");
  });

  it("absent params collapse onto the same key as empty params", () => {
    expect(deploymentKeys.list()).toEqual(deploymentKeys.list({}));
  });
});

describe("listDeployments", () => {
  it("calls the deployments URL and returns the items array", async () => {
    const items = [makeSummary({ deployment_id: "a" }), makeSummary({ deployment_id: "b" })];
    const spy = vi
      .spyOn(client, "apiFetch")
      .mockResolvedValue({ items, total: 2 } as never);

    const result = await listDeployments({ status: "running,paused" });

    expect(spy).toHaveBeenCalledWith(
      "/api/live/deployments?status=running%2Cpaused",
    );
    expect(result).toHaveLength(2);
    expect(result[0]!.deployment_id).toBe("a");
  });

  it("returns an empty array when the envelope has no items", async () => {
    vi.spyOn(client, "apiFetch").mockResolvedValue({ items: [], total: 0 } as never);
    const result = await listDeployments();
    expect(result).toEqual([]);
  });

  it("preserves null capital fields verbatim (no fabricated 0)", async () => {
    const item = makeSummary({
      deployment_id: "z",
      unrealized_pnl_usd: null,
      deployed_capital_usd: null,
    });
    vi.spyOn(client, "apiFetch").mockResolvedValue({ items: [item], total: 1 } as never);
    const result = await listDeployments();
    expect(result[0]!.unrealized_pnl_usd).toBeNull();
    expect(result[0]!.deployed_capital_usd).toBeNull();
  });
});
