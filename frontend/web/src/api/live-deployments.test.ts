import { afterEach, describe, expect, it, vi } from "vitest";

import * as client from "./client";
import {
  buildDeploymentsListUrl,
  deploymentKeys,
  LIVE_SSE_EVENTS,
  listDeployments,
  openDeploymentStream,
  parseMetricsTick,
  type DeploymentStreamEvent,
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
    daily_loss_budget_usd: null,
    stop_at: null,
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

  // bead s78.2: the home passes the last-visit boundary as `?since` so the
  // backend can count REAL risk vetoes since that boundary.
  it("encodes the since boundary as an rfc3339 query param", () => {
    const url = buildDeploymentsListUrl({ since: "2026-06-13T00:00:00.000Z" });
    expect(url).toContain("since=2026-06-13T00%3A00%3A00.000Z");
  });

  it("omits since when absent (first visit ⇒ no boundary ⇒ field stays null)", () => {
    expect(buildDeploymentsListUrl({ status: "running" })).toBe(
      "/api/live/deployments?status=running",
    );
  });

  it("omits an empty since string", () => {
    expect(buildDeploymentsListUrl({ since: "" })).toBe(
      "/api/live/deployments",
    );
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

  // bead s78.2: the since boundary is part of the cache key so changing the
  // last-visit boundary refetches (and so the home's since-bearing key is
  // distinct from a since-free one).
  it("folds the since boundary into the cache key", () => {
    const withSince = deploymentKeys.list({ since: "2026-06-13T00:00:00Z" });
    const withoutSince = deploymentKeys.list({});
    expect(withSince).not.toEqual(withoutSince);
    expect(withSince).toContain("2026-06-13T00:00:00Z");
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

  it("forwards the since boundary to the fetched URL", async () => {
    const spy = vi
      .spyOn(client, "apiFetch")
      .mockResolvedValue({ items: [], total: 0 } as never);

    await listDeployments({ status: "running", since: "2026-06-13T00:00:00.000Z" });

    expect(spy).toHaveBeenCalledWith(
      "/api/live/deployments?status=running&since=2026-06-13T00%3A00%3A00.000Z",
    );
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

// ─── SSE: parseMetricsTick (the dual-shape `metrics` channel) ────────────────

describe("parseMetricsTick", () => {
  it("parses the flat capital tick, keeping present fields", () => {
    const patch = parseMetricsTick({
      time: 1_700_000_000,
      equity_usd: 10_500,
      drawdown_pct: 2.5,
      deployed_capital_usd: 3_000,
      unrealized_pnl_usd: 120,
      realized_pnl_usd: 380,
      daily_loss_limit_remaining_usd: 450,
      n_trades: 4,
    });
    expect(patch).toEqual({
      time: 1_700_000_000,
      equity_usd: 10_500,
      drawdown_pct: 2.5,
      deployed_capital_usd: 3_000,
      unrealized_pnl_usd: 120,
      realized_pnl_usd: 380,
      daily_loss_limit_remaining_usd: 450,
      n_trades: 4,
    });
  });

  // HONESTY: an omitted capital field stays UNDEFINED (poll wins), never 0.
  it("leaves omitted capital fields undefined (no fabricated 0)", () => {
    const patch = parseMetricsTick({ time: 1, equity_usd: 10_000, n_trades: 0 });
    expect(patch).not.toBeNull();
    expect(patch!.equity_usd).toBe(10_000);
    expect(patch!.n_trades).toBe(0);
    expect("unrealized_pnl_usd" in patch!).toBe(false);
    expect("deployed_capital_usd" in patch!).toBe(false);
    expect("drawdown_pct" in patch!).toBe(false);
    expect(patch!.unrealized_pnl_usd).toBeUndefined();
  });

  // The bare equity heartbeat (tagged envelope) maps to a metrics patch that
  // carries ONLY equity — the capital fields fall back to the poll (degrade).
  it("parses the equity-only tagged envelope into an equity-only patch", () => {
    const patch = parseMetricsTick({
      event: "equity",
      data: { time: 7, equity_usd: 99 },
    });
    expect(patch).toEqual({ time: 7, equity_usd: 99 });
    expect("unrealized_pnl_usd" in patch!).toBe(false);
  });

  it("returns null for an empty / unrecognizable object", () => {
    expect(parseMetricsTick({})).toBeNull();
    expect(parseMetricsTick(null)).toBeNull();
    expect(parseMetricsTick("nope")).toBeNull();
  });
});

// ─── SSE: openDeploymentStream ───────────────────────────────────────────────

type Listener = (ev: MessageEvent) => void;

/// Minimal EventSource double capturing constructor URLs + registered
/// listeners so a test can drive frames synchronously.
function installMockEventSource() {
  const ctorUrls: string[] = [];
  const listeners: Record<string, Listener[]> = {};
  const closed: boolean[] = [];
  class MockES {
    url: string;
    private idx: number;
    constructor(url: string) {
      this.url = url;
      this.idx = closed.length;
      closed.push(false);
      ctorUrls.push(url);
    }
    addEventListener(name: string, fn: EventListener) {
      (listeners[name] ??= []).push(fn as unknown as Listener);
    }
    removeEventListener() {}
    close() {
      closed[this.idx] = true;
    }
  }
  const original = globalThis.EventSource;
  (globalThis as { EventSource: unknown }).EventSource =
    MockES as unknown as typeof EventSource;

  function fire(name: string, payload: unknown) {
    const data = typeof payload === "string" ? payload : JSON.stringify(payload);
    const ev = new MessageEvent(name, { data });
    for (const fn of listeners[name] ?? []) fn(ev);
  }
  function restore() {
    (globalThis as { EventSource: unknown }).EventSource = original;
  }
  return { ctorUrls, listeners, closed, fire, restore };
}

describe("openDeploymentStream", () => {
  it("opens an EventSource against /api/live/deployments/:id/stream", () => {
    const es = installMockEventSource();
    try {
      const close = openDeploymentStream("dep-7", () => {});
      expect(es.ctorUrls).toEqual(["/api/live/deployments/dep-7/stream"]);
      close();
    } finally {
      es.restore();
    }
  });

  it("encodes the deployment id in the URL", () => {
    const es = installMockEventSource();
    try {
      const close = openDeploymentStream("a/b id", () => {});
      expect(es.ctorUrls[0]).toBe("/api/live/deployments/a%2Fb%20id/stream");
      close();
    } finally {
      es.restore();
    }
  });

  it("registers a listener for every LIVE_SSE_EVENTS name", () => {
    const es = installMockEventSource();
    try {
      const close = openDeploymentStream("dep-1", () => {});
      for (const name of LIVE_SSE_EVENTS) {
        expect(es.listeners[name]?.length ?? 0).toBeGreaterThan(0);
      }
      close();
    } finally {
      es.restore();
    }
  });

  it("maps a snapshot frame to a typed snapshot event", () => {
    const es = installMockEventSource();
    const received: DeploymentStreamEvent[] = [];
    try {
      const close = openDeploymentStream("dep-1", (ev) => received.push(ev));
      es.fire("snapshot", makeSummary({ deployment_id: "dep-1" }));
      expect(received).toHaveLength(1);
      expect(received[0]!.event).toBe("snapshot");
      expect(
        (received[0]!.data as LiveDeploymentSummary).deployment_id,
      ).toBe("dep-1");
      close();
    } finally {
      es.restore();
    }
  });

  it("maps a flat capital metrics frame to a normalized metrics patch", () => {
    const es = installMockEventSource();
    const received: DeploymentStreamEvent[] = [];
    try {
      const close = openDeploymentStream("dep-1", (ev) => received.push(ev));
      es.fire("metrics", {
        time: 1,
        equity_usd: 10_500,
        unrealized_pnl_usd: 73.25,
        n_trades: 2,
      });
      expect(received).toHaveLength(1);
      const ev = received[0]!;
      expect(ev.event).toBe("metrics");
      if (ev.event !== "metrics") throw new Error("expected metrics");
      expect(ev.data.unrealized_pnl_usd).toBe(73.25);
      expect(ev.data.equity_usd).toBe(10_500);
      // Omitted capital fields stay undefined (poll wins) — no fabricated 0.
      expect("deployed_capital_usd" in ev.data).toBe(false);
      close();
    } finally {
      es.restore();
    }
  });

  it("maps the equity-only heartbeat envelope onto the metrics channel", () => {
    const es = installMockEventSource();
    const received: DeploymentStreamEvent[] = [];
    try {
      const close = openDeploymentStream("dep-1", (ev) => received.push(ev));
      es.fire("metrics", { event: "equity", data: { time: 3, equity_usd: 42 } });
      expect(received).toHaveLength(1);
      const ev = received[0]!;
      if (ev.event !== "metrics") throw new Error("expected metrics");
      expect(ev.data.equity_usd).toBe(42);
      // The heartbeat carries no capital — they degrade to the poll value.
      expect("unrealized_pnl_usd" in ev.data).toBe(false);
      close();
    } finally {
      es.restore();
    }
  });

  it("drops malformed frames without emitting", () => {
    const es = installMockEventSource();
    const received: DeploymentStreamEvent[] = [];
    try {
      const close = openDeploymentStream("dep-1", (ev) => received.push(ev));
      es.fire("metrics", "{not json");
      es.fire("snapshot", "{also bad");
      // A subsequent valid lagged frame still flows.
      es.fire("lagged", { dropped: 3 });
      expect(received.map((e) => e.event)).toEqual(["lagged"]);
      expect((received[0]!.data as { dropped: number }).dropped).toBe(3);
      close();
    } finally {
      es.restore();
    }
  });

  it("closes the EventSource on the returned handle (no leak)", () => {
    const es = installMockEventSource();
    try {
      const close = openDeploymentStream("dep-1", () => {});
      expect(es.closed[0]).toBe(false);
      close();
      expect(es.closed[0]).toBe(true);
    } finally {
      es.restore();
    }
  });
});
