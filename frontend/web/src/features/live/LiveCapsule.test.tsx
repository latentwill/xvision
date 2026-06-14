// frontend/web/src/features/live/LiveCapsule.test.tsx
import { afterEach, describe, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import type { ReactElement } from "react";
import { MemoryRouter } from "react-router-dom";
import { LiveCapsule, type LiveCapsuleProps } from "./LiveCapsule";
import type { EvalCapsuleFocused } from "../agent-runs/EvalCapsule";
import type { RunSpan } from "@/api/types-agent-runs";

// The capsule renders `<Link>` for the short-tag → live inspector. Wrap renders
// in a MemoryRouter so the router context is present in jsdom.
function renderCapsule(node: ReactElement) {
  return render(<MemoryRouter>{node}</MemoryRouter>);
}

function focused(overrides: Partial<EvalCapsuleFocused> = {}): EvalCapsuleFocused {
  return {
    id: "live_run_1",
    kind: "live",
    short: "mr·sol",
    status: "eval",
    spans: 12,
    elapsed: "0:43",
    cost: "$0.18",
    pnl: "+1.42%",
    currentSpan: null,
    ...overrides,
  };
}

function brokerSpan(
  id: string,
  detail: Partial<NonNullable<RunSpan["broker_call"]>>,
): RunSpan {
  return {
    span_id: id,
    parent_span_id: null,
    name: "broker.call",
    kind: "broker.call",
    started_at: "2026-06-13T14:00:00Z",
    finished_at: "2026-06-13T14:00:01Z",
    status: "ok",
    attributes: {},
    broker_call: {
      side: "buy",
      symbol: "SOL-PERP",
      qty: 1.5,
      intended_price: 142.5,
      order_type: "market",
      venue: "live",
      idempotency_key: null,
      outcome: "filled",
      fill_price: 142.6,
      fill_qty: 1.5,
      fee: 0.01,
      broker_order_id: "ord_1",
      error_class: null,
      error_message: null,
      severity: null,
      ...detail,
    },
  } as RunSpan;
}

function renderLive(overrides: Partial<LiveCapsuleProps> = {}) {
  return renderCapsule(
    <LiveCapsule
      run={focused()}
      brokerSpans={[
        brokerSpan("bs-1", {
          side: "buy",
          symbol: "SOL-PERP",
          qty: 1.5,
          outcome: "filled",
        }),
        brokerSpan("bs-2", {
          side: "sell",
          symbol: "BTC-PERP",
          qty: 0.25,
          intended_price: 64000,
          outcome: "rejected",
        }),
      ]}
      {...overrides}
    />,
  );
}

afterEach(() => cleanup());

describe("LiveCapsule", () => {
  test("renders the focused live run line with LIVE prefix", () => {
    renderLive();
    const cap = screen.getByTestId("live-capsule");
    expect(within(cap).getByTestId("capsule-kind-label")).toHaveTextContent(
      "LIVE",
    );
    expect(within(cap).getByText("mr·sol")).toBeInTheDocument();
  });

  test("short-tag links to /live/runs/<id>", () => {
    renderLive({ run: focused({ id: "run_live_xyz" }) });
    const link = screen.getByRole("link", { name: /open eval run mr·sol/i });
    expect(link).toHaveAttribute("href", "/live/runs/run_live_xyz");
  });

  test("renders one order row per broker.call span (side/symbol/qty/price/outcome)", () => {
    renderLive();
    const orders = screen.getByTestId("live-capsule-orders");
    const rows = within(orders).getAllByTestId("live-capsule-order");
    expect(rows).toHaveLength(2);

    // First order: BUY SOL-PERP filled.
    expect(rows[0]).toHaveTextContent(/BUY/i);
    expect(rows[0]).toHaveTextContent("SOL-PERP");
    expect(rows[0]).toHaveTextContent("1.5");
    expect(rows[0]).toHaveTextContent(/filled/i);

    // Second order: SELL BTC-PERP rejected at the intended price.
    expect(rows[1]).toHaveTextContent(/SELL/i);
    expect(rows[1]).toHaveTextContent("BTC-PERP");
    expect(rows[1]).toHaveTextContent(/rejected/i);
    expect(rows[1]).toHaveTextContent("64000");
  });

  test("renders the venue as-is from the span (no hardcoding)", () => {
    renderLive({
      brokerSpans: [
        brokerSpan("bs-1", { venue: "live", symbol: "SOL-PERP" }),
      ],
    });
    const rows = screen.getAllByTestId("live-capsule-order");
    expect(rows[0]).toHaveTextContent("live");
  });

  test("status pill pulses while running, terminal when finished", () => {
    const api = renderLive({ run: focused({ status: "eval" }) });
    // Running → the leading status dot pulses.
    const cap = screen.getByTestId("live-capsule");
    expect(cap.querySelector(".animate-pulse")).not.toBeNull();

    cleanup();
    // Finished/terminal → no pulse.
    renderLive({ run: focused({ status: "pass" }) });
    const cap2 = screen.getByTestId("live-capsule");
    expect(cap2.querySelector(".animate-pulse")).toBeNull();
    void api;
  });

  test("renders an empty-orders hint when there are no broker.call spans", () => {
    renderLive({ brokerSpans: [] });
    const orders = screen.getByTestId("live-capsule-orders");
    expect(within(orders).queryAllByTestId("live-capsule-order")).toHaveLength(0);
    expect(orders).toHaveTextContent(/no orders/i);
  });

  test("invokes onExpandDock and onPopOut from the trailing controls", () => {
    const onExpandDock = vi.fn();
    const onPopOut = vi.fn();
    renderLive({ onExpandDock, onPopOut });
    fireEvent.click(screen.getByRole("button", { name: /expand trace dock/i }));
    fireEvent.click(
      screen.getByRole("button", { name: /open dedicated trace view/i }),
    );
    expect(onExpandDock).toHaveBeenCalledTimes(1);
    expect(onPopOut).toHaveBeenCalledTimes(1);
  });

  test("renders the focused run's currentSpan chip while live", () => {
    renderLive({
      run: focused({
        currentSpan: {
          color: "#7dd3fc",
          label: "BROKER",
          name: "broker.call SOL-PERP",
          elapsed: "120ms",
        },
      }),
    });
    expect(screen.getByText("BROKER")).toBeInTheDocument();
    expect(screen.getByText("broker.call SOL-PERP")).toBeInTheDocument();
    expect(screen.getByText("120ms")).toBeInTheDocument();
  });

  test("tones every order outcome incl. cancelled and in-progress (null)", () => {
    renderLive({
      brokerSpans: [
        brokerSpan("o-filled", { outcome: "filled", symbol: "SOL-PERP" }),
        brokerSpan("o-cancelled", { outcome: "cancelled", symbol: "ETH-PERP" }),
        brokerSpan("o-rejected", { outcome: "rejected", symbol: "BTC-PERP" }),
        // In-progress order: started, not yet finished → no outcome.
        brokerSpan("o-inflight", { outcome: null, symbol: "AVAX-PERP" }),
      ],
    });
    const rows = screen.getAllByTestId("live-capsule-order");
    expect(rows).toHaveLength(4);
    // The in-progress order renders without an outcome label / crash.
    expect(rows[3]).toHaveTextContent("AVAX-PERP");
  });

  test("renders the fidelity badge inline inside the focused content row", () => {
    renderLive({ retentionMode: "full_debug" });
    const cap = screen.getByTestId("live-capsule");
    const badge = within(cap).getByTestId("capsule-fidelity-badge");
    expect(badge.textContent).toContain("full");

    // Inline on the focused row (carrying the LIVE prefix), not a stacked row.
    const kindLabel = within(cap).getByTestId("capsule-kind-label");
    const contentRow = kindLabel.closest("div.transition-colors");
    expect(contentRow).not.toBeNull();
    expect(contentRow!.contains(badge)).toBe(true);
  });

  test("omits the fidelity badge when no retentionMode is provided", () => {
    renderLive();
    expect(screen.queryByTestId("capsule-fidelity-badge")).toBeNull();
  });

  test("error and warn run statuses tint the capsule border (no crash)", () => {
    renderLive({ run: focused({ status: "error" }) });
    expect(screen.getByTestId("live-capsule")).toBeInTheDocument();
    cleanup();
    renderLive({ run: focused({ status: "warn" }) });
    expect(screen.getByTestId("live-capsule")).toBeInTheDocument();
  });
});
