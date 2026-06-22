// frontend/web/src/features/home/deploy-readiness.test.ts
//
// TDD spec for the pure deploy-readiness selector (xvision-e17). Covers the
// pass / fail / unknown outcomes for each of the three minimum checks, with
// special emphasis on the broker-configured-but-UNREACHABLE case which the
// spec (§7.1 panel 6) classifies as an explicit deploy-blocker FAIL, not an
// "unknown".
//
// HONESTY MANDATE: the selector reasons only over honest config facts
// (provider key presence, broker reachability, safety-pause, in-flight run
// staleness). It NEVER derives or fabricates P&L / capital / budget numbers.

import { describe, expect, it } from "vitest";

import type { BrokersReport, ProviderRow, RunSummary } from "@/api/types.gen";
import type { AlpacaTestReport } from "@/api/types.gen";
import type { SafetyStateResponse } from "@/api/safety";

import {
  buildDeployReadiness,
  DEPLOY_READINESS_STUCK_MS,
  type DeployReadinessInput,
} from "./deploy-readiness";

// ─── fixtures ────────────────────────────────────────────────────────────────

function provider(over: Partial<ProviderRow> = {}): ProviderRow {
  return {
    name: "anthropic",
    kind: "anthropic",
    base_url: "https://api.anthropic.com",
    api_key_env: "ANTHROPIC_API_KEY",
    api_key_set: true,
    synthetic: false,
    is_default: true,
    enabled_models: ["claude-opus-4"],
    ...over,
  };
}

function broker(over: Partial<BrokersReport["alpaca"]> = {}): BrokersReport["alpaca"] {
  return {
    name: "Alpaca",
    kind: "alpaca",
    credentials: [],
    configured: true,
    stored: true,
    stored_key_id_suffix: "AB12",
    base_url: "https://paper-api.alpaca.markets",
    note: "paper trading",
    ...over,
  };
}

function brokers(over: Partial<BrokersReport> = {}): BrokersReport {
  return {
    alpaca: broker(),
    orderly: broker({ name: "Orderly Network", kind: "orderly", configured: false }),
    byreal: broker({ name: "Byreal", kind: "byreal", configured: false }),
    byreal_spot: broker({ name: "Byreal Spot", kind: "byreal_spot", configured: false }),
    degen_arena: broker({ name: "Degen Arena", kind: "degen_arena", configured: false }),
    hyperliquid: broker({ name: "Hyperliquid", kind: "hyperliquid", configured: false }),
    ...over,
  };
}

function brokerTest(over: Partial<AlpacaTestReport> = {}): AlpacaTestReport {
  return {
    ok: true,
    latency_ms: 120,
    account_status: "ACTIVE",
    equity: "100000",
    error: null,
    ...over,
  };
}

function run(over: Partial<RunSummary> = {}): RunSummary {
  return {
    id: "run_1",
    agent_id: "agent_1",
    scenario_id: "scn_1",
    strategy: null,
    scenario: null,
    mode: "backtest",
    status: "running",
    started_at: new Date().toISOString(),
    completed_at: null,
    sharpe: null,
    max_drawdown_pct: null,
    total_return_pct: null,
    error: null,
    actual_input_tokens: null,
    actual_output_tokens: null,
    inference_cost_quote_total: null,
    net_return_pct: null,
    filter_summaries: [],
    auto_fire_review: false,
    review_model: null,
    max_annotations_per_review: null,
    paused: false,
    paused_at: null,
    flatten_requested: false,
    ...over,
  } as RunSummary;
}

function safety(over: Partial<SafetyStateResponse> = {}): SafetyStateResponse {
  return { paused: false, paused_at: null, paused_by: null, reason: null, ...over };
}

// A fully-green input baseline; individual tests override one axis at a time.
function greenInput(over: Partial<DeployReadinessInput> = {}): DeployReadinessInput {
  return {
    providers: [provider()],
    brokers: brokers(),
    brokerTest: brokerTest(),
    safety: safety(),
    inflightRuns: [],
    nowMs: Date.parse("2026-06-13T12:00:00Z"),
    ...over,
  };
}

function byId(checks: ReturnType<typeof buildDeployReadiness>, id: string) {
  const c = checks.find((x) => x.id === id);
  if (!c) throw new Error(`no check with id=${id} (got ${checks.map((x) => x.id).join(",")})`);
  return c;
}

// ─── shape / ordering ────────────────────────────────────────────────────────

describe("buildDeployReadiness — shape", () => {
  it("returns the three minimum checks in stable order: keys, broker, eval", () => {
    const checks = buildDeployReadiness(greenInput());
    expect(checks.map((c) => c.id)).toEqual(["keys", "broker", "no-blocking-eval"]);
  });

  it("every check carries id, label, status, detail", () => {
    for (const c of buildDeployReadiness(greenInput())) {
      expect(typeof c.id).toBe("string");
      expect(typeof c.label).toBe("string");
      expect(["pass", "fail", "unknown"]).toContain(c.status);
      expect(typeof c.detail).toBe("string");
    }
  });

  it("uses plain literal labels (keys / broker / no blocking eval)", () => {
    const checks = buildDeployReadiness(greenInput());
    expect(byId(checks, "keys").label).toBe("keys");
    expect(byId(checks, "broker").label).toBe("broker");
    expect(byId(checks, "no-blocking-eval").label).toBe("no blocking eval");
  });
});

// ─── provider keys check ─────────────────────────────────────────────────────

describe("buildDeployReadiness — provider keys", () => {
  it("PASS when at least one non-synthetic provider has its key set", () => {
    expect(byId(buildDeployReadiness(greenInput()), "keys").status).toBe("pass");
  });

  it("FAIL when a configured provider is missing its key", () => {
    const checks = buildDeployReadiness(
      greenInput({ providers: [provider({ api_key_set: false })] }),
    );
    const keys = byId(checks, "keys");
    expect(keys.status).toBe("fail");
    expect(keys.detail).toMatch(/ANTHROPIC_API_KEY|missing/i);
    expect(keys.link?.to).toBe("/settings/providers");
  });

  it("ignores synthetic + no-auth providers when judging missing keys", () => {
    // synthetic row and a no-auth (empty api_key_env) row must not trip a FAIL
    const checks = buildDeployReadiness(
      greenInput({
        providers: [
          provider(),
          provider({ name: "synthetic", synthetic: true, api_key_set: false }),
          provider({ name: "local", api_key_env: "", api_key_set: false }),
        ],
      }),
    );
    expect(byId(checks, "keys").status).toBe("pass");
  });

  it("UNKNOWN when providers have not loaded yet (undefined)", () => {
    const checks = buildDeployReadiness(greenInput({ providers: undefined }));
    expect(byId(checks, "keys").status).toBe("unknown");
  });

  it("FAIL when there are zero usable providers at all", () => {
    const checks = buildDeployReadiness(greenInput({ providers: [] }));
    expect(byId(checks, "keys").status).toBe("fail");
  });
});

// ─── broker check (configured AND reachable) ─────────────────────────────────

describe("buildDeployReadiness — broker", () => {
  it("PASS when broker is configured AND the test connection succeeded", () => {
    expect(byId(buildDeployReadiness(greenInput()), "broker").status).toBe("pass");
  });

  it("UNKNOWN when no broker is configured (nothing to deploy against yet)", () => {
    const checks = buildDeployReadiness(
      greenInput({ brokers: brokers({ alpaca: broker({ configured: false }) }), brokerTest: undefined }),
    );
    const b = byId(checks, "broker");
    expect(b.status).toBe("unknown");
    expect(b.link?.to).toBe("/settings/brokers");
  });

  it("UNKNOWN when broker is configured but the connection test has not run yet", () => {
    const checks = buildDeployReadiness(greenInput({ brokerTest: undefined }));
    expect(byId(checks, "broker").status).toBe("unknown");
  });

  it("FAIL (deploy-blocker) when broker is configured but UNREACHABLE", () => {
    // §7.1 panel 6: configured-but-unreachable is an explicit blocker, NOT unknown.
    const checks = buildDeployReadiness(
      greenInput({ brokerTest: brokerTest({ ok: false, account_status: null, equity: null, error: "401 unauthorized" }) }),
    );
    const b = byId(checks, "broker");
    expect(b.status).toBe("fail");
    expect(b.detail).toMatch(/unreachable|401 unauthorized/i);
    expect(b.link?.to).toBe("/settings/brokers");
  });

  it("UNKNOWN when brokers report itself has not loaded (undefined)", () => {
    const checks = buildDeployReadiness(greenInput({ brokers: undefined }));
    expect(byId(checks, "broker").status).toBe("unknown");
  });
});

// ─── no-blocking-eval check (not paused AND no stuck >2h run) ─────────────────

describe("buildDeployReadiness — no blocking eval", () => {
  it("PASS when not safety-paused and no in-flight run is stuck", () => {
    const checks = buildDeployReadiness(
      greenInput({
        inflightRuns: [run({ started_at: new Date(Date.parse("2026-06-13T11:30:00Z")).toISOString() })],
      }),
    );
    expect(byId(checks, "no-blocking-eval").status).toBe("pass");
  });

  it("FAIL when safety is paused", () => {
    const checks = buildDeployReadiness(
      greenInput({ safety: safety({ paused: true, reason: "manual" }) }),
    );
    const c = byId(checks, "no-blocking-eval");
    expect(c.status).toBe("fail");
    expect(c.detail).toMatch(/paused/i);
    expect(c.link?.to).toBe("/safety");
  });

  it("FAIL when an in-flight run has been running > 2h (stuck)", () => {
    const nowMs = Date.parse("2026-06-13T12:00:00Z");
    const checks = buildDeployReadiness(
      greenInput({
        nowMs,
        inflightRuns: [
          run({ status: "running", started_at: new Date(nowMs - DEPLOY_READINESS_STUCK_MS - 1000).toISOString() }),
        ],
      }),
    );
    const c = byId(checks, "no-blocking-eval");
    expect(c.status).toBe("fail");
    expect(c.detail).toMatch(/stuck|running/i);
    expect(c.link?.to).toBe("/eval-runs");
  });

  it("does NOT flag a queued run that has waited > 2h (only running runs go stuck)", () => {
    const nowMs = Date.parse("2026-06-13T12:00:00Z");
    const checks = buildDeployReadiness(
      greenInput({
        nowMs,
        inflightRuns: [
          run({ status: "queued", started_at: new Date(nowMs - DEPLOY_READINESS_STUCK_MS - 1000).toISOString() }),
        ],
      }),
    );
    expect(byId(checks, "no-blocking-eval").status).toBe("pass");
  });

  it("treats a run started exactly at the threshold as not-yet-stuck", () => {
    const nowMs = Date.parse("2026-06-13T12:00:00Z");
    const checks = buildDeployReadiness(
      greenInput({
        nowMs,
        inflightRuns: [
          run({ status: "running", started_at: new Date(nowMs - DEPLOY_READINESS_STUCK_MS).toISOString() }),
        ],
      }),
    );
    expect(byId(checks, "no-blocking-eval").status).toBe("pass");
  });

  it("UNKNOWN when safety state has not loaded yet", () => {
    const checks = buildDeployReadiness(greenInput({ safety: undefined }));
    expect(byId(checks, "no-blocking-eval").status).toBe("unknown");
  });

  it("prefers the paused FAIL over a stuck-run FAIL but still FAILs either way", () => {
    const nowMs = Date.parse("2026-06-13T12:00:00Z");
    const checks = buildDeployReadiness(
      greenInput({
        nowMs,
        safety: safety({ paused: true }),
        inflightRuns: [
          run({ status: "running", started_at: new Date(nowMs - DEPLOY_READINESS_STUCK_MS - 1).toISOString() }),
        ],
      }),
    );
    expect(byId(checks, "no-blocking-eval").status).toBe("fail");
  });
});

// ─── honesty: no fabricated money numbers ────────────────────────────────────

describe("buildDeployReadiness — honesty", () => {
  it("never emits a $ / P&L / capital figure in any detail string", () => {
    const checks = buildDeployReadiness(
      greenInput({ brokerTest: brokerTest({ equity: "100000" }) }),
    );
    for (const c of checks) {
      expect(c.detail).not.toMatch(/\$/);
      expect(c.detail).not.toMatch(/100000/);
      expect(c.detail).not.toMatch(/P&?L|capital|equity|budget/i);
    }
  });
});
