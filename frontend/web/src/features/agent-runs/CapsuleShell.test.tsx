// frontend/web/src/features/agent-runs/CapsuleShell.test.tsx
//
// Covers the shared CapsuleRow hover affordance. The eval/live capsule render
// paths are regression-gated by EvalCapsule.test.tsx + LiveCapsule.test.tsx;
// the only behavior unique to the extracted primitive is the non-focused-row
// hover background swap (and its focused-row guard), exercised here.
import { afterEach, describe, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { CapsuleRow, type EvalCapsuleRow } from "./CapsuleShell";

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
