import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MemoryRouter } from "react-router-dom";

import { NagStrip } from "./NagStrip";
import type { AttentionItem } from "./NagStrip";
import { failedRunNags } from "@/features/home/failed-runs";
import type { RunSummary } from "@/api/types.gen";

function renderStrip(items: AttentionItem[]) {
  return render(
    <MemoryRouter>
      <NagStrip items={items} />
    </MemoryRouter>,
  );
}

const PROVIDER_ITEM: AttentionItem = {
  tone: "warn",
  title: "1 provider missing API key",
  detail: "OpenAI → OPENAI_API_KEY",
  link: { to: "/settings/providers", label: "configure" },
};

const BROKER_ITEM: AttentionItem = {
  tone: "warn",
  title: "Alpaca credentials not set",
  detail: "ALPACA_API_KEY, ALPACA_SECRET_KEY",
  link: { to: "/settings/brokers", label: "set up" },
};

afterEach(() => {
  cleanup();
});

describe("NagStrip", () => {
  it("renders missing-provider-key nag item", () => {
    renderStrip([PROVIDER_ITEM]);

    expect(screen.getByTestId("nag-strip")).toBeInTheDocument();
    expect(screen.getByText("1 provider missing API key")).toBeInTheDocument();
    expect(screen.getByText(/OpenAI → OPENAI_API_KEY/)).toBeInTheDocument();
  });

  it("renders broker-unconfigured nag item", () => {
    renderStrip([BROKER_ITEM]);

    expect(screen.getByTestId("nag-strip")).toBeInTheDocument();
    expect(screen.getByText("Alpaca credentials not set")).toBeInTheDocument();
    expect(screen.getByText(/ALPACA_API_KEY/)).toBeInTheDocument();
  });

  it("returns null when items array is empty", () => {
    const { container } = renderStrip([]);

    expect(screen.queryByTestId("nag-strip")).not.toBeInTheDocument();
    expect(container.firstChild).toBeNull();
  });

  it("shows first 3 items when more than 3 provided and shows '+ N more' toggle", () => {
    const items: AttentionItem[] = [
      { tone: "warn", title: "Nag one", detail: "detail 1" },
      { tone: "warn", title: "Nag two", detail: "detail 2" },
      { tone: "warn", title: "Nag three", detail: "detail 3" },
      { tone: "info", title: "Nag four", detail: "detail 4" },
      { tone: "danger", title: "Nag five", detail: "detail 5" },
    ];

    renderStrip(items);

    // First 3 are visible
    expect(screen.getByText("Nag one")).toBeInTheDocument();
    expect(screen.getByText("Nag two")).toBeInTheDocument();
    expect(screen.getByText("Nag three")).toBeInTheDocument();

    // 4th and 5th are NOT visible yet
    expect(screen.queryByText("Nag four")).not.toBeInTheDocument();
    expect(screen.queryByText("Nag five")).not.toBeInTheDocument();

    // "+ 2 more" toggle is shown (5 - 3 = 2)
    expect(screen.getByText(/\+ 2 more/i)).toBeInTheDocument();
  });

  it("clicking '+ N more' expands to show all items inline (no modal)", async () => {
    const user = userEvent.setup();
    const items: AttentionItem[] = [
      { tone: "warn", title: "Nag one", detail: "detail 1" },
      { tone: "warn", title: "Nag two", detail: "detail 2" },
      { tone: "warn", title: "Nag three", detail: "detail 3" },
      { tone: "info", title: "Nag four", detail: "detail 4" },
      { tone: "danger", title: "Nag five", detail: "detail 5" },
    ];

    renderStrip(items);

    // Before expand: only 3 shown
    expect(screen.queryByText("Nag four")).not.toBeInTheDocument();

    // Collapsed toggle exposes aria-expanded=false (accessible disclosure)
    const toggle = screen.getByText(/\+ 2 more/i);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(toggle).toHaveAttribute("aria-controls", "nag-strip-items");
    await user.click(toggle);

    // After expanding, the "show less" toggle reports aria-expanded=true
    expect(screen.getByText(/show less/i)).toHaveAttribute(
      "aria-expanded",
      "true",
    );

    // After expand: all 5 visible inline — no dialog/modal
    expect(screen.getByText("Nag one")).toBeInTheDocument();
    expect(screen.getByText("Nag two")).toBeInTheDocument();
    expect(screen.getByText("Nag three")).toBeInTheDocument();
    expect(screen.getByText("Nag four")).toBeInTheDocument();
    expect(screen.getByText("Nag five")).toBeInTheDocument();

    // No modal or dialog role rendered
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    // The toggle text changes to "show less" or disappears
    expect(screen.queryByText(/\+ 2 more/i)).not.toBeInTheDocument();
  });

  it("renders tone dot indicators — warn=amber, danger=red, info=blue", () => {
    const items: AttentionItem[] = [
      { tone: "warn", title: "Warn item", detail: "warn detail" },
      { tone: "danger", title: "Danger item", detail: "danger detail" },
      { tone: "info", title: "Info item", detail: "info detail" },
    ];

    renderStrip(items);

    // Each item has a data-tone attribute on its dot
    const warnDots = document.querySelectorAll("[data-tone='warn']");
    const dangerDots = document.querySelectorAll("[data-tone='danger']");
    const infoDots = document.querySelectorAll("[data-tone='info']");

    expect(warnDots.length).toBeGreaterThanOrEqual(1);
    expect(dangerDots.length).toBeGreaterThanOrEqual(1);
    expect(infoDots.length).toBeGreaterThanOrEqual(1);
  });

  it("renders link when item has a link property", () => {
    renderStrip([PROVIDER_ITEM]);

    const link = screen.getByRole("link", { name: /configure/i });
    expect(link).toBeInTheDocument();
    expect(link).toHaveAttribute("href", "/settings/providers");
  });

  // ─── stale-infra-failure nag rows (bead xvision-1zs) ───────────────────────

  it("renders a stale-infra-failure nag routed to the run, with config nags after it", () => {
    const NOW = Date.parse("2026-06-13T12:00:00Z");
    const staleIso = new Date(NOW - 3 * 60 * 60 * 1000).toISOString();

    const failedRun: RunSummary = {
      id: "run-stale-1",
      agent_id: "a-1",
      scenario_id: "sc-1",
      strategy: { id: "s-1", display_name: "Momentum V2" },
      scenario: null,
      mode: "backtest",
      status: "failed",
      started_at: staleIso,
      completed_at: staleIso,
      sharpe: null,
      max_drawdown_pct: null,
      total_return_pct: null,
      error: "Connection refused (os error 61)",
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
    };
    unrealized_pnl_usd: null,
    skipped_dispatches: 0,
    delayed_decisions: 0,
    forced_cancels: 0,

    // Composition mirrors home.tsx: infra nags FIRST, config nags after.
    const items = [...failedRunNags([failedRun], NOW), PROVIDER_ITEM];
    renderStrip(items);

    // Infra nag row routes to the run with a "view run" link.
    const viewRun = screen.getByRole("link", { name: /view run/i });
    expect(viewRun).toHaveAttribute("href", "/eval-runs/run-stale-1");
    expect(screen.getByText(/Momentum V2 run failed/i)).toBeInTheDocument();

    // Config nag still renders after the infra nag.
    expect(screen.getByText("1 provider missing API key")).toBeInTheDocument();
    expect(
      screen.getByRole("link", { name: /configure/i }),
    ).toBeInTheDocument();

    // Calm: warn tone dot, never danger, for the infra nag.
    expect(document.querySelectorAll("[data-tone='danger']").length).toBe(0);
  });
});
