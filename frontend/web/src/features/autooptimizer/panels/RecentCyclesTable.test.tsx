import { describe, expect, it, vi, afterEach } from "vitest";
import { fireEvent, screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { RecentCyclesTable } from "./RecentCyclesTable";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

type CycleRow = {
  cycle_id: string;
  strategy_id?: string | null;
  node_count: number;
  active_count: number;
  rejected_count: number;
  first_created_at: string;
  last_created_at: string;
  cost_usd: number | null;
  input_tokens: number | null;
  output_tokens: number | null;
  unpriced_calls: number | null;
};

function row(overrides: Partial<CycleRow> & { cycle_id: string }): CycleRow {
  return {
    node_count: 5,
    active_count: 2,
    rejected_count: 3,
    first_created_at: "2026-06-01T00:00:00Z",
    last_created_at: "2026-06-01T01:00:00Z",
    cost_usd: 4.2,
    input_tokens: 1000,
    output_tokens: 500,
    unpriced_calls: 0,
    ...overrides,
  };
}

describe("RecentCyclesTable", () => {
  it("links each cycle row to its detail screen", async () => {
    vi.spyOn(client, "apiFetch").mockResolvedValue([
      row({ cycle_id: "cyc-1", strategy_id: "strat-abc" }),
    ]);
    renderWithProviders(<RecentCyclesTable />);
    const link = await screen.findByRole("link", { name: /cyc-1/ });
    expect(link).toHaveAttribute("href", "/optimizer/cycle/cyc-1");
  });

  // QA: the cycle history needs a Strategy column. The value is sourced from
  // the authoritative session bridge (events → session → strategy_id), not the
  // stale lineage metadata an earlier attempt rejected — so it links to the
  // strategy detail when present.
  it("shows a Strategy column linking to the strategy detail", async () => {
    vi.spyOn(client, "apiFetch").mockResolvedValue([
      row({ cycle_id: "cyc-1", strategy_id: "strat-abc" }),
    ]);
    renderWithProviders(<RecentCyclesTable />);
    await screen.findByRole("link", { name: /cyc-1/ });
    expect(
      screen.getByRole("columnheader", { name: "Strategy" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "strat-abc" })).toHaveAttribute(
      "href",
      "/strategies/strat-abc",
    );
  });

  it("renders a cycle with no strategy as an em-dash (no link)", async () => {
    vi.spyOn(client, "apiFetch").mockResolvedValue([
      row({ cycle_id: "cyc-nostrat", strategy_id: null }),
    ]);
    renderWithProviders(<RecentCyclesTable />);
    await screen.findByRole("link", { name: /cyc-nostrat/ });
    // Column still present; the cell is a non-link placeholder (no /strategies link).
    expect(
      screen.getByRole("columnheader", { name: "Strategy" }),
    ).toBeInTheDocument();
    const hasStrategyLink = screen
      .getAllByRole("link")
      .some((l) => l.getAttribute("href")?.startsWith("/strategies/"));
    expect(hasStrategyLink).toBe(false);
  });

  // UI3: the history list must not render unbounded rows — it caps at the
  // default page and reveals the rest behind a "Show all" affordance.
  it("caps visible rows and reveals the rest via Show all", async () => {
    const rows = Array.from({ length: 30 }, (_, i) =>
      row({ cycle_id: `cyc-${i}`, strategy_id: "s" }),
    );
    vi.spyOn(client, "apiFetch").mockResolvedValue(rows);
    renderWithProviders(<RecentCyclesTable />);

    // Wait for the first row, then assert the 26th row is initially hidden.
    await screen.findByRole("link", { name: /^cyc-0$/ });
    expect(screen.queryByRole("link", { name: /^cyc-25$/ })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: /show all/i }));
    expect(screen.getByRole("link", { name: /^cyc-25$/ })).toBeInTheDocument();
  });

  // UI4: a CLI-launched cycle persists to lineage (no live SSE, no session) and
  // must still appear in the history list, which reads the persisted
  // /api/autooptimizer/cycles route — not the live event stream.
  it("renders a persisted CLI cycle with no session/SSE", async () => {
    vi.spyOn(client, "apiFetch").mockResolvedValue([
      row({ cycle_id: "cli-cycle", strategy_id: null, cost_usd: null }),
    ]);
    renderWithProviders(<RecentCyclesTable />);
    expect(
      await screen.findByRole("link", { name: /cli-cycle/ }),
    ).toHaveAttribute("href", "/optimizer/cycle/cli-cycle");
  });
});
