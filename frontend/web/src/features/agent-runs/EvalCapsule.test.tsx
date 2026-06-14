// frontend/web/src/features/agent-runs/EvalCapsule.test.tsx
import { afterEach, describe, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import type { ReactElement } from "react";
import { MemoryRouter } from "react-router-dom";
import {
  EvalCapsule,
  type EvalCapsuleFocused,
  type EvalCapsuleRow,
} from "./EvalCapsule";

// The capsule renders `<Link>` for F-6 (clickable short-tag → eval inspector).
// Wrap renders in a MemoryRouter so the router context is present in jsdom.
function renderCapsule(node: ReactElement) {
  return render(<MemoryRouter>{node}</MemoryRouter>);
}

function rerenderCapsule(
  api: ReturnType<typeof render>,
  node: ReactElement,
) {
  return api.rerender(<MemoryRouter>{node}</MemoryRouter>);
}

function focused(overrides: Partial<EvalCapsuleFocused> = {}): EvalCapsuleFocused {
  return {
    id: "abc1234",
    short: "mr·flash",
    status: "eval",
    spans: 47,
    elapsed: "0:43",
    cost: "$0.18",
    currentSpan: null,
    ...overrides,
  };
}

function sibling(overrides: Partial<EvalCapsuleRow> = {}): EvalCapsuleRow {
  return {
    id: "sib-1",
    short: "mom·opex",
    status: "eval",
    spans: 12,
    elapsed: "0:38",
    cost: "$0.11",
    ...overrides,
  };
}

afterEach(() => cleanup());

describe("EvalCapsule", () => {
  test("renders a single focused row with no toggle when siblings empty", () => {
    renderCapsule(<EvalCapsule focused={focused()} siblings={[]} />);

    const cap = screen.getByTestId("run-status-strip");
    expect(within(cap).queryByText(/OTHER/)).toBeNull();
    expect(within(cap).getByText("mr·flash")).toBeInTheDocument();
  });

  test("shows the +N · OTHERS toggle and reveals siblings when expanded", () => {
    renderCapsule(
      <EvalCapsule
        focused={focused()}
        siblings={[
          sibling({ id: "s1", short: "mom·opex" }),
          sibling({ id: "s2", short: "vol·vix", status: "warn" }),
        ]}
      />,
    );

    const cap = screen.getByTestId("run-status-strip");
    const toggle = within(cap).getByRole("button", { name: /show 2 other evals/i });
    expect(toggle).toHaveAttribute("aria-expanded", "false");

    // Sibling rows hidden when collapsed.
    expect(within(cap).queryByText("mom·opex")).toBeNull();

    fireEvent.click(toggle);
    expect(within(cap).getByText("mom·opex")).toBeInTheDocument();
    expect(within(cap).getByText("vol·vix")).toBeInTheDocument();
    expect(
      within(cap).getByRole("button", { name: /collapse other evals/i }),
    ).toHaveAttribute("aria-expanded", "true");
  });

  test("auto-opens the capsule when a sibling error appears", () => {
    const api = renderCapsule(
      <EvalCapsule
        focused={focused()}
        siblings={[sibling({ id: "s1", short: "mom·opex" })]}
      />,
    );
    expect(screen.queryByText("mom·opex")).toBeNull();

    rerenderCapsule(
      api,
      <EvalCapsule
        focused={focused()}
        siblings={[
          sibling({ id: "s1", short: "mom·opex" }),
          sibling({ id: "s2", short: "liq·fed", status: "error" }),
        ]}
      />,
    );

    // Errored sibling promoted to the top of the stack + auto-expanded.
    expect(screen.getByText("liq·fed")).toBeInTheDocument();
    expect(screen.getByText("mom·opex")).toBeInTheDocument();
    // ERR badge surfaces when collapsed; once expanded it stays inside the
    // stack rather than on the toggle, so we only assert the row order:
    const rows = screen
      .getAllByRole("link", { name: /open eval run/i })
      .map((a) => a.textContent ?? "");
    const errIdx = rows.findIndex((t) => t.includes("liq·fed"));
    const momIdx = rows.findIndex((t) => t.includes("mom·opex"));
    expect(errIdx).toBeLessThan(momIdx);
  });

  test("clicking a sibling row's surrounding button invokes onSwitchFocus", () => {
    // F-6 (qa-round-7): the short `strategy·scenario` tag is now a Link
    // that routes to the run inspector — clicking THAT no longer fires
    // onSwitchFocus (it stopPropagations to navigate cleanly). The rest
    // of the row (status pill, span/elapsed/cost text) still triggers
    // focus-switch via the wrapping button, which is the preserved
    // secondary affordance.
    const onSwitchFocus = vi.fn();
    renderCapsule(
      <EvalCapsule
        focused={focused()}
        siblings={[sibling({ id: "s1", short: "mom·opex", spans: 12 })]}
        onSwitchFocus={onSwitchFocus}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /show 1 other eval/i }));
    fireEvent.click(
      screen.getByRole("button", { name: /switch focus to eval run mom·opex/i }),
    );
    expect(onSwitchFocus).toHaveBeenCalledTimes(1);
    expect(onSwitchFocus.mock.calls[0]![0]!.id).toBe("s1");
  });

  test("short tag is a Link that routes to /eval-runs/<id>", () => {
    // F-6 (qa-round-7): assert the Link target so the click-through to the
    // inspector is wired regardless of focused-vs-sibling state.
    const onSwitchFocus = vi.fn();
    renderCapsule(
      <EvalCapsule
        focused={focused({ id: "run_focused", short: "mr·flash" })}
        siblings={[sibling({ id: "run_sibling", short: "mom·opex" })]}
        onSwitchFocus={onSwitchFocus}
      />,
    );
    // Focused short tag — always visible.
    const focusedLink = screen.getByRole("link", { name: /open eval run mr·flash/i });
    expect(focusedLink).toHaveAttribute("href", "/eval-runs/run_focused");
    expect(focusedLink.closest("button")).toBeNull();

    // Expand, then check the sibling tag too.
    fireEvent.click(screen.getByRole("button", { name: /show 1 other eval/i }));
    const siblingLink = screen.getByRole("link", { name: /open eval run mom·opex/i });
    expect(siblingLink).toHaveAttribute("href", "/eval-runs/run_sibling");
    expect(siblingLink.closest("button")).toBeNull();

    // Clicking the link should NOT trigger focus-switch (stopPropagation).
    fireEvent.click(siblingLink);
    expect(onSwitchFocus).not.toHaveBeenCalled();
  });

  test("prefix label reads EVAL by default and LIVE (gold tint) for kind=live", () => {
    const api = renderCapsule(<EvalCapsule focused={focused()} siblings={[]} />);
    expect(screen.getByTestId("capsule-kind-label")).toHaveTextContent("EVAL");

    rerenderCapsule(
      api,
      <EvalCapsule focused={focused({ kind: "live" })} siblings={[]} />,
    );
    const label = screen.getByTestId("capsule-kind-label");
    expect(label).toHaveTextContent("LIVE");
    expect(label).toHaveStyle({ color: "var(--gold)" });
  });

  test("kind=live focused row's short tag routes to /live/runs/<id>", () => {
    renderCapsule(
      <EvalCapsule
        focused={focused({ id: "run_live1", kind: "live" })}
        siblings={[]}
      />,
    );
    const link = screen.getByRole("link", { name: /open eval run mr·flash/i });
    expect(link).toHaveAttribute("href", "/live/runs/run_live1");
  });

  test("renders an N ERR chip on the collapsed toggle when siblings have errors", () => {
    renderCapsule(
      <EvalCapsule
        focused={focused()}
        siblings={[
          sibling({ id: "s1", short: "ok·one" }),
          sibling({ id: "s2", short: "err·one", status: "error" }),
          sibling({ id: "s3", short: "err·two", status: "error" }),
        ]}
      />,
    );
    // Because the test mounts with errors already present, the auto-open
    // effect won't fire (the lastErrorCount ref is seeded on mount), so the
    // capsule starts collapsed and the ERR count is visible on the toggle.
    expect(screen.getByText(/2 ERR/)).toBeInTheDocument();
  });

  test("renders the fidelity badge inline inside the focused content row (not as a stacked row)", () => {
    renderCapsule(
      <EvalCapsule focused={focused()} siblings={[]} retentionMode="full_debug" />,
    );
    const cap = screen.getByTestId("run-status-strip");
    const badge = within(cap).getByTestId("capsule-fidelity-badge");
    expect(badge.textContent).toContain("full");

    // The badge must be a descendant of the focused content row (the same row
    // carrying the EVAL prefix + short tag), so it sits on the single
    // horizontal line rather than stacking above the body as an empty row.
    const kindLabel = within(cap).getByTestId("capsule-kind-label");
    const contentRow = kindLabel.closest("div.transition-colors");
    expect(contentRow).not.toBeNull();
    expect(contentRow!.contains(badge)).toBe(true);
  });

  test("omits the fidelity badge when no retentionMode is provided", () => {
    renderCapsule(<EvalCapsule focused={focused()} siblings={[]} />);
    expect(screen.queryByTestId("capsule-fidelity-badge")).toBeNull();
  });

  test("renders the focused row's currentSpan chip", () => {
    renderCapsule(
      <EvalCapsule
        focused={focused({
          currentSpan: {
            color: "#7dd3fc",
            label: "MODEL",
            name: "model.call claude-haiku",
            elapsed: "880ms",
          },
        })}
        siblings={[]}
      />,
    );
    expect(screen.getByText("MODEL")).toBeInTheDocument();
    expect(screen.getByText("model.call claude-haiku")).toBeInTheDocument();
    expect(screen.getByText("880ms")).toBeInTheDocument();
  });
});
