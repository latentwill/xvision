// frontend/web/src/features/agent-runs/EvalCapsule.test.tsx
import { afterEach, describe, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import {
  EvalCapsule,
  type EvalCapsuleFocused,
  type EvalCapsuleRow,
} from "./EvalCapsule";

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
    render(<EvalCapsule focused={focused()} siblings={[]} />);

    const cap = screen.getByTestId("run-status-strip");
    expect(within(cap).queryByText(/OTHER/)).toBeNull();
    expect(within(cap).getByText("mr·flash")).toBeInTheDocument();
  });

  test("shows the +N · OTHERS toggle and reveals siblings when expanded", () => {
    render(
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
    const { rerender } = render(
      <EvalCapsule
        focused={focused()}
        siblings={[sibling({ id: "s1", short: "mom·opex" })]}
      />,
    );
    expect(screen.queryByText("mom·opex")).toBeNull();

    rerender(
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
    const rows = screen.getAllByRole("button").map((b) => b.textContent ?? "");
    const errIdx = rows.findIndex((t) => t.includes("liq·fed"));
    const momIdx = rows.findIndex((t) => t.includes("mom·opex"));
    expect(errIdx).toBeLessThan(momIdx);
  });

  test("clicking a sibling row invokes onSwitchFocus with that run", () => {
    const onSwitchFocus = vi.fn();
    render(
      <EvalCapsule
        focused={focused()}
        siblings={[sibling({ id: "s1", short: "mom·opex" })]}
        onSwitchFocus={onSwitchFocus}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /show 1 other eval/i }));
    fireEvent.click(screen.getByText("mom·opex"));
    expect(onSwitchFocus).toHaveBeenCalledTimes(1);
    expect(onSwitchFocus.mock.calls[0]![0]!.id).toBe("s1");
  });

  test("renders an N ERR chip on the collapsed toggle when siblings have errors", () => {
    render(
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

  test("renders the focused row's currentSpan chip", () => {
    render(
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
