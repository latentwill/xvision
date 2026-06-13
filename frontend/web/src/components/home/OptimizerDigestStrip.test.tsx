import { describe, expect, it, vi, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { OptimizerDigestStrip } from "./OptimizerDigestStrip";
import * as apiModule from "@/features/autooptimizer/api";
import type { SessionListItem, StatsRow } from "@/features/autooptimizer/api";

afterEach(() => vi.restoreAllMocks());

/** Mock the stats query (zn2 FE-derivable segments). Defaults to no rows so the
 *  pre-existing session-only tests keep rendering em-dash placeholders. */
function mockStats(rows: StatsRow[] = []) {
  vi.spyOn(apiModule, "useOptimizerStats").mockReturnValue({
    data: rows,
    isLoading: false,
    isError: false,
    isPending: false,
    isSuccess: true,
  } as unknown as ReturnType<typeof apiModule.useOptimizerStats>);
}

function statRow(over: Partial<StatsRow>): StatsRow {
  return {
    cycle_id: "c1",
    session_id: "s1",
    ts: "2026-06-12T10:00:00Z",
    kept: 1,
    suspect: 0,
    dropped: 1,
    best_delta_holdout: null,
    cost_usd: 0.1,
    cum_cost_usd: 0.5,
    ...over,
  };
}

function renderStrip() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <OptimizerDigestStrip />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const baseSession: SessionListItem & { suspect_count?: number } = {
  session_id: "sess_01TESTABCDEF",
  strategy_id: "strat-abc",
  state: "finished",
  mode: "explore",
  cycles_completed: 12,
  kept_count: 5,
  suspect_count: 2,
  cost_usd: 0.47,
  finished_at: "2026-06-07T10:00:00Z",
};

describe("OptimizerDigestStrip", () => {
  // Test 1: Returns null when sessions list is empty
  it("returns null when sessions list is empty", () => {
    mockStats();
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
      isPending: false,
      isSuccess: true,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);

    const { container } = renderStrip();
    expect(container.firstChild).toBeNull();
  });

  // Test 2: Returns null while sessions are loading (undefined data)
  it("returns null while sessions are loading", () => {
    mockStats();
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: undefined,
      isLoading: true,
      isError: false,
      isPending: true,
      isSuccess: false,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);

    const { container } = renderStrip();
    expect(container.firstChild).toBeNull();
  });

  // Test 3: Renders "X experiments · Y kept · Z suspect" from fixture SessionListItem
  it("renders experiments, kept, and suspect counts from the most recent session", () => {
    mockStats();
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [baseSession] as SessionListItem[],
      isLoading: false,
      isError: false,
      isPending: false,
      isSuccess: true,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);

    renderStrip();
    const strip = screen.getByTestId("optimizer-digest-strip");
    expect(strip.textContent).toContain("12 experiments");
    expect(strip.textContent).toContain("5 kept");
    expect(strip.textContent).toContain("2 suspect");
  });

  // Test 4: Shows cost formatted to 2 decimal places
  it("shows cost formatted to 2 decimal places", () => {
    mockStats();
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [baseSession] as SessionListItem[],
      isLoading: false,
      isError: false,
      isPending: false,
      isSuccess: true,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);

    renderStrip();
    const strip = screen.getByTestId("optimizer-digest-strip");
    expect(strip.textContent).toContain("$0.47");
  });

  // Test 4b: Shows "?" when cost_usd is undefined
  it("shows '?' when cost_usd is undefined", () => {
    mockStats();
    const sessionNoCost: SessionListItem = { ...baseSession, cost_usd: undefined };
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [sessionNoCost] as SessionListItem[],
      isLoading: false,
      isError: false,
      isPending: false,
      isSuccess: true,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);

    renderStrip();
    const strip = screen.getByTestId("optimizer-digest-strip");
    expect(strip.textContent).toContain("$?");
  });

  // Test 5: "Honesty check" text present (exact string, not "canary")
  it("shows 'Honesty check' text and never uses 'canary'", () => {
    mockStats();
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [baseSession] as SessionListItem[],
      isLoading: false,
      isError: false,
      isPending: false,
      isSuccess: true,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);

    renderStrip();
    const strip = screen.getByTestId("optimizer-digest-strip");
    expect(strip.textContent).toContain("Honesty check");
    expect(strip.textContent).not.toContain("canary");
  });

  // Test 6: Link to /optimizer/run/:session_id present
  it("renders a link to /optimizer/run/:session_id", () => {
    mockStats();
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [baseSession] as SessionListItem[],
      isLoading: false,
      isError: false,
      isPending: false,
      isSuccess: true,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);

    renderStrip();
    const link = screen.getByRole("link", { name: /view run/i });
    expect(link).toHaveAttribute("href", "/optimizer/run/sess_01TESTABCDEF");
  });

  // ─── S0 / O1a + O1b: real suspect + honesty rendering ─────────────────────

  function mockSession(session: SessionListItem, stats: StatsRow[] = []) {
    mockStats(stats);
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [session] as SessionListItem[],
      isLoading: false,
      isError: false,
      isPending: false,
      isSuccess: true,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);
  }

  it("renders the real suspect_count from the typed field (O1a)", () => {
    mockSession({ ...baseSession, suspect_count: 4 });
    renderStrip();
    expect(screen.getByTestId("optimizer-digest-strip").textContent).toContain("4 suspect");
  });

  it("shows 'Honesty check ✓' when the latest cycle passed (O1b)", () => {
    mockSession({ ...baseSession, honesty_passed: true });
    renderStrip();
    expect(screen.getByTestId("optimizer-digest-strip").textContent).toContain("Honesty check ✓");
  });

  it("shows 'Honesty check ✗ failed' when the latest cycle failed (O1b)", () => {
    mockSession({ ...baseSession, honesty_passed: false });
    renderStrip();
    expect(screen.getByTestId("optimizer-digest-strip").textContent).toContain(
      "Honesty check ✗ failed",
    );
  });

  it("shows 'Honesty check —' when no honesty signal is present (O1b)", () => {
    mockSession({ ...baseSession, honesty_passed: undefined });
    renderStrip();
    expect(screen.getByTestId("optimizer-digest-strip").textContent).toContain("Honesty check —");
  });

  // ─── zn2: FE-derivable digest slices (acceptance / holdout Δ / cost) ───────

  it("renders a 30d acceptance-rate segment derived from stats rows", () => {
    mockSession(baseSession, [
      statRow({ ts: "2026-06-10T00:00:00Z", kept: 3, suspect: 1, dropped: 0 }),
      statRow({ ts: "2026-06-11T00:00:00Z", kept: 1, suspect: 0, dropped: 4 }),
    ]);
    renderStrip();
    const strip = screen.getByTestId("optimizer-digest-strip");
    // 4 kept / 9 total ≈ 44%
    expect(strip.textContent).toContain("44% accepted (30d)");
  });

  it("renders an em-dash acceptance segment when no in-window cycles produced candidates", () => {
    mockSession(baseSession, []);
    renderStrip();
    const seg = screen.getByTestId("optimizer-digest-acceptance");
    expect(seg.textContent).toContain("— accepted (30d)");
  });

  it("tones the acceptance segment as warn when the recent half degraded", () => {
    mockSession(baseSession, [
      statRow({ ts: "2026-06-01T00:00:00Z", kept: 4, suspect: 0, dropped: 0 }),
      statRow({ ts: "2026-06-02T00:00:00Z", kept: 4, suspect: 0, dropped: 0 }),
      statRow({ ts: "2026-06-03T00:00:00Z", kept: 4, suspect: 0, dropped: 0 }),
      statRow({ ts: "2026-06-04T00:00:00Z", kept: 4, suspect: 0, dropped: 0 }),
      statRow({ ts: "2026-06-10T00:00:00Z", kept: 0, suspect: 0, dropped: 4 }),
      statRow({ ts: "2026-06-11T00:00:00Z", kept: 0, suspect: 0, dropped: 4 }),
      statRow({ ts: "2026-06-12T00:00:00Z", kept: 0, suspect: 0, dropped: 4 }),
      statRow({ ts: "2026-06-13T00:00:00Z", kept: 0, suspect: 0, dropped: 4 }),
    ]);
    renderStrip();
    const seg = screen.getByTestId("optimizer-digest-acceptance");
    expect(seg.className).toMatch(/text-warn/);
    // honesty doc on the segment title — may name the sabotage/null-result test…
    expect(seg.getAttribute("title")?.toLowerCase()).toMatch(/sabotag|null[- ]result/);
    // …but the word "canary" never appears in visible copy.
    expect(seg.textContent).not.toContain("canary");
  });

  it("renders the best holdout Δ from stats rows, gold when positive", () => {
    mockSession(baseSession, [
      statRow({ best_delta_holdout: 0.12 }),
      statRow({ best_delta_holdout: 0.41 }),
    ]);
    renderStrip();
    const seg = screen.getByTestId("optimizer-digest-holdout");
    expect(seg.textContent).toContain("Best holdout Δ +0.41");
    expect(seg.className).toMatch(/text-gold/);
  });

  it("tones the holdout Δ as warn when the best is negative", () => {
    mockSession(baseSession, [statRow({ best_delta_holdout: -0.2 })]);
    renderStrip();
    const seg = screen.getByTestId("optimizer-digest-holdout");
    expect(seg.textContent).toContain("Best holdout Δ -0.20");
    expect(seg.className).toMatch(/text-warn/);
  });

  it("renders an em-dash holdout segment when no cycle carries a delta", () => {
    mockSession(baseSession, [statRow({ best_delta_holdout: null })]);
    renderStrip();
    const seg = screen.getByTestId("optimizer-digest-holdout");
    expect(seg.textContent).toContain("Best holdout Δ —");
  });

  it("tints the cost segment as warn when the latest cycle cost is anomalous", () => {
    mockSession(baseSession, [
      statRow({ ts: "2026-06-09T00:00:00Z", cost_usd: 0.1 }),
      statRow({ ts: "2026-06-10T00:00:00Z", cost_usd: 0.12 }),
      statRow({ ts: "2026-06-11T00:00:00Z", cost_usd: 0.11 }),
      statRow({ ts: "2026-06-12T00:00:00Z", cost_usd: 0.09 }),
      statRow({ ts: "2026-06-13T00:00:00Z", cost_usd: 0.8 }),
    ]);
    renderStrip();
    const seg = screen.getByTestId("optimizer-digest-cost");
    expect(seg.className).toMatch(/text-warn/);
    expect(seg.getAttribute("title")?.toLowerCase()).toMatch(/cost|spend|median/);
  });

  it("does not tint the cost segment when the latest cost is in line with trailing cycles", () => {
    mockSession(baseSession, [
      statRow({ ts: "2026-06-11T00:00:00Z", cost_usd: 0.1 }),
      statRow({ ts: "2026-06-12T00:00:00Z", cost_usd: 0.12 }),
      statRow({ ts: "2026-06-13T00:00:00Z", cost_usd: 0.11 }),
    ]);
    renderStrip();
    const seg = screen.getByTestId("optimizer-digest-cost");
    expect(seg.className).not.toMatch(/text-warn/);
  });

  it("renders the spend with an em-dash budget denominator placeholder (cap deferred to 8wn)", () => {
    // baseSession.cost_usd === 0.47 (the honest session spend numerator);
    // the cap denominator is deferred to 8wn → em-dash, never a faked cap.
    mockSession(baseSession, [statRow({ cost_usd: 0.33 })]);
    renderStrip();
    const seg = screen.getByTestId("optimizer-digest-cost");
    expect(seg.textContent).toContain("$0.47 / —");
  });

  it("never renders the word 'canary' anywhere even with stats present", () => {
    mockSession(baseSession, [
      statRow({ best_delta_holdout: 0.4, kept: 2, dropped: 1 }),
    ]);
    renderStrip();
    expect(screen.getByTestId("optimizer-digest-strip").textContent).not.toContain("canary");
  });
});
