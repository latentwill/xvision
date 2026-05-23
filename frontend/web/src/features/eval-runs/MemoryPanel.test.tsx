// Tests for the memory-aware MemoryPanel — the eval-run surface that
// pairs each per-decision recall list with the new
// `memory_recalled_into_*` findings inline.
//
// Coverage:
// - renders nothing when there are no recalls and no findings
// - renders a recall block for each `memory_recall` event
// - matches a `memory_recalled_into_bad_decision` warning to its
//   parent recall by `decision_index` and renders it inline
// - the inline finding row names the memory item id(s) responsible
// - info-severity good-outcome finding renders distinct from warning
// - orphan findings (no matching recall in `events`) still surface

import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";

import { MemoryPanel, type MemoryAwareFinding } from "./MemoryPanel";

const recall = {
  kind: "memory_recall",
  payload: {
    namespace: "agent:01HZTRADER",
    decision_id: 4,
    items: [
      {
        id: "stale-pattern-01",
        score: 0.85,
        text_preview: "RSI cross at 30 → reversal (last seen 2024-08)",
      },
    ],
  },
};

const warningFinding: MemoryAwareFinding = {
  id: "f1",
  kind: "memory_recalled_into_bad_decision",
  severity: "warning",
  summary:
    "Decision 4 (bad outcome, pnl=-25.0000) had 1 memory item(s) recalled into its briefing: stale-pattern-01",
  description:
    "Decision 4 closed with a losing realized pnl (-25.0000) and was driven by 1 recalled memory item(s): [stale-pattern-01].",
  recommendation:
    "Open the named memory items in the workspace memory store; demote or rewrite patterns that no longer hold.",
  evidence: {
    decision_index: 4,
    memory_item_ids: ["stale-pattern-01"],
    outcome: "bad",
  },
};

const goodFinding: MemoryAwareFinding = {
  id: "f2",
  kind: "memory_recalled_into_good_decision",
  severity: "info",
  summary:
    "Decision 1 (good outcome, pnl=+40.0000) had 1 memory item(s) recalled into its briefing: helpful-pattern-01",
  description: "Surfaced because the operator opted in.",
  evidence: {
    decision_index: 1,
    memory_item_ids: ["helpful-pattern-01"],
    outcome: "good",
  },
};

afterEach(() => {
  cleanup();
});

describe("eval-runs MemoryPanel — base rendering", () => {
  it("renders nothing when there are no recalls and no findings", () => {
    const { container } = render(<MemoryPanel events={[]} findings={[]} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders a recall block when only recalls are present", () => {
    render(<MemoryPanel events={[recall]} findings={[]} />);
    expect(screen.getByTestId("memory-panel")).toBeInTheDocument();
    expect(screen.getByText(/Decision 4/)).toBeInTheDocument();
    expect(screen.getByText(/agent:01HZTRADER/)).toBeInTheDocument();
    expect(
      screen.getByText(/RSI cross at 30 → reversal/),
    ).toBeInTheDocument();
  });

  it("ignores non-memory events", () => {
    const { container } = render(
      <MemoryPanel
        events={[
          { kind: "tool_call", payload: { name: "broker.submit" } },
          { kind: "model_call_finished", payload: { provider: "anthropic" } },
        ]}
        findings={[]}
      />,
    );
    expect(container).toBeEmptyDOMElement();
  });
});

describe("eval-runs MemoryPanel — inline finding display", () => {
  it("renders a warning finding inline under its matching recall (by decision_index)", () => {
    render(<MemoryPanel events={[recall]} findings={[warningFinding]} />);

    const panel = screen.getByTestId("memory-panel");
    // The warning finding row must appear inside the panel.
    const findingRow = within(panel).getByRole("alert");
    expect(findingRow).toHaveAttribute(
      "data-finding-kind",
      "memory_recalled_into_bad_decision",
    );
    expect(findingRow).toHaveAttribute("data-finding-severity", "warning");
    // The summary text must surface the memory item id so an operator
    // can trace from the finding back to the responsible pattern. The
    // id appears both inside the summary text and as a code chip — both
    // are inside the row, so we accept either.
    expect(
      within(findingRow).getAllByText(/stale-pattern-01/).length,
    ).toBeGreaterThan(0);
  });

  it("renders the responsible memory item ids as code chips on the finding row", () => {
    render(<MemoryPanel events={[recall]} findings={[warningFinding]} />);
    const chips = screen.getAllByTestId("memory-finding-item-id");
    // Exactly one chip for the single recalled item.
    expect(chips).toHaveLength(1);
    expect(chips[0]!.textContent).toBe("stale-pattern-01");
  });

  it("renders an info-severity good-outcome finding distinct from a warning", () => {
    const goodRecall = {
      kind: "memory_recall",
      payload: {
        namespace: "global",
        decision_id: 1,
        items: [
          {
            id: "helpful-pattern-01",
            score: 0.82,
            text_preview: "after consolidation, breakout",
          },
        ],
      },
    };
    render(<MemoryPanel events={[goodRecall]} findings={[goodFinding]} />);
    const panel = screen.getByTestId("memory-panel");
    // Info-severity findings use role=status, not alert.
    const findingRow = within(panel).getByRole("status");
    expect(findingRow).toHaveAttribute(
      "data-finding-kind",
      "memory_recalled_into_good_decision",
    );
    expect(findingRow).toHaveAttribute("data-finding-severity", "info");
    expect(
      within(findingRow).getAllByText(/helpful-pattern-01/).length,
    ).toBeGreaterThan(0);
  });

  it("does not render findings of unrelated kinds", () => {
    const otherFinding: MemoryAwareFinding = {
      id: "f-other",
      kind: "regime_fit_mismatch",
      severity: "warning",
      summary: "unrelated finding",
    };
    render(<MemoryPanel events={[recall]} findings={[otherFinding]} />);
    // Only the recall block renders — no alert / status finding rows.
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("surfaces orphan findings (no matching recall) at the bottom", () => {
    // Caller provides a finding for decision 4, but no recall event
    // for decision 4 — the panel still surfaces the finding so it
    // can't get silently dropped.
    render(<MemoryPanel events={[]} findings={[warningFinding]} />);
    const orphans = screen.getByTestId("memory-orphan-findings");
    expect(orphans).toBeInTheDocument();
    expect(within(orphans).getByRole("alert")).toBeInTheDocument();
  });
});
