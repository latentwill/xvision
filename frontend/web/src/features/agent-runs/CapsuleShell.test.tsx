// frontend/web/src/features/agent-runs/CapsuleShell.test.tsx
//
// Covers the shared CapsuleRow hover affordance. The eval/live capsule render
// paths are regression-gated by EvalCapsule.test.tsx + LiveCapsule.test.tsx;
// the only behavior unique to the extracted primitive is the non-focused-row
// hover background swap (and its focused-row guard), exercised here.
import { afterEach, describe, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { CapsuleRow, CapsuleShell, FidelityBadge, type EvalCapsuleRow } from "./CapsuleShell";
import type { RetentionMode } from "../../api/types-agent-runs";

function row(overrides: Partial<EvalCapsuleRow> = {}): EvalCapsuleRow {
  return {
    id: "r1",
    kind: "eval",
    short: "mr·sol",
    status: "eval",
    spans: 3,
    elapsed: "0:10",
    cost: "$0.02",
    ...overrides,
  };
}

function rowEl(container: HTMLElement): HTMLElement {
  const el = container.querySelector("div.transition-colors");
  if (!(el instanceof HTMLElement)) throw new Error("row element not found");
  return el;
}

afterEach(() => cleanup());

describe("CapsuleRow hover", () => {
  test("a non-focused row swaps its background on mouse enter/leave", () => {
    const { container } = render(
      <MemoryRouter>
        <CapsuleRow run={row()} focused={false} onClick={vi.fn()} />
      </MemoryRouter>,
    );
    const el = rowEl(container);
    fireEvent.mouseEnter(el);
    expect(el.style.background).toContain("surface-hover");
    fireEvent.mouseLeave(el);
    expect(el.style.background).toContain("transparent");
  });

  test("a focused row ignores hover (the !focused guard)", () => {
    const { container } = render(
      <MemoryRouter>
        <CapsuleRow run={row()} focused />
      </MemoryRouter>,
    );
    const el = rowEl(container);
    const before = el.style.background;
    fireEvent.mouseEnter(el);
    fireEvent.mouseLeave(el);
    expect(el.style.background).toBe(before);
  });
});

describe("FidelityBadge", () => {
  const cases: Array<[RetentionMode, string]> = [
    ["hash_only", "hash-only"],
    ["redacted", "redacted"],
    ["full_debug", "full"],
  ];

  test.each(cases)("renders %s as label %s", (mode, label) => {
    const { getByTestId } = render(<FidelityBadge retentionMode={mode} />);
    const badge = getByTestId("capsule-fidelity-badge");
    expect(badge.textContent).toContain(label);
    expect(badge.getAttribute("data-fidelity")).toBe(mode);
  });

  test("title describes whether bodies are present", () => {
    const { getByTestId, rerender } = render(<FidelityBadge retentionMode="hash_only" />);
    expect(getByTestId("capsule-fidelity-badge").getAttribute("title")).toMatch(/no.*bod/i);
    rerender(<FidelityBadge retentionMode="full_debug" />);
    expect(getByTestId("capsule-fidelity-badge").getAttribute("title")).toMatch(/bod/i);
  });
});

describe("CapsuleShell fidelity", () => {
  test("renders the fidelity badge when retentionMode is provided", () => {
    const { getByTestId } = render(
      <CapsuleShell testId="shell" tone="eval" borderColor="var(--gold)" retentionMode="redacted">
        <div>body</div>
      </CapsuleShell>,
    );
    const badge = getByTestId("capsule-fidelity-badge");
    expect(badge.textContent).toContain("redacted");
  });

  test("omits the fidelity badge when retentionMode is absent", () => {
    const { queryByTestId } = render(
      <CapsuleShell testId="shell" tone="eval" borderColor="var(--gold)">
        <div>body</div>
      </CapsuleShell>,
    );
    expect(queryByTestId("capsule-fidelity-badge")).toBeNull();
  });
});
