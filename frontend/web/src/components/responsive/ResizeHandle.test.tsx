import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";

import { ResizeHandle } from "./ResizeHandle";

beforeEach(() => {
  cleanup();
});

afterEach(() => {
  cleanup();
});

describe("ResizeHandle", () => {
  test("renders a hidden-on-mobile col-resize strip", () => {
    const { container } = render(<ResizeHandle onDelta={() => {}} />);
    const handle = container.firstElementChild as HTMLElement;
    // hidden on small screens, visible on lg+
    expect(handle.className).toContain("hidden");
    expect(handle.className).toContain("lg:block");
    expect(handle.className).toContain("cursor-col-resize");
    expect(handle.style.width).toBe("4px");
  });

  test("calls onDelta with the mousemove delta after mousedown", () => {
    const onDelta = vi.fn();
    const { container } = render(<ResizeHandle onDelta={onDelta} />);
    const handle = container.firstElementChild as HTMLElement;

    fireEvent.mouseDown(handle, { clientX: 300 });
    fireEvent.mouseMove(document, { clientX: 340 });
    fireEvent.mouseUp(document);

    expect(onDelta).toHaveBeenCalledWith(40);
  });

  test("accumulates delta across multiple mousemove events", () => {
    const deltas: number[] = [];
    const { container } = render(<ResizeHandle onDelta={(d) => deltas.push(d)} />);
    const handle = container.firstElementChild as HTMLElement;

    fireEvent.mouseDown(handle, { clientX: 200 });
    fireEvent.mouseMove(document, { clientX: 210 }); // +10
    fireEvent.mouseMove(document, { clientX: 225 }); // +15
    fireEvent.mouseUp(document);

    expect(deltas).toEqual([10, 15]);
  });

  test("stops reporting deltas after mouseup", () => {
    const onDelta = vi.fn();
    const { container } = render(<ResizeHandle onDelta={onDelta} />);
    const handle = container.firstElementChild as HTMLElement;

    fireEvent.mouseDown(handle, { clientX: 100 });
    fireEvent.mouseUp(document);
    fireEvent.mouseMove(document, { clientX: 200 });

    expect(onDelta).not.toHaveBeenCalled();
  });

  test("prevents default on mousedown to avoid text selection", () => {
    const { container } = render(<ResizeHandle onDelta={() => {}} />);
    const handle = container.firstElementChild as HTMLElement;

    const event = new MouseEvent("mousedown", { clientX: 100, bubbles: true, cancelable: true });
    handle.dispatchEvent(event);

    expect(event.defaultPrevented).toBe(true);
  });

  test("renders the inner visual indicator line", () => {
    const { container } = render(<ResizeHandle onDelta={() => {}} />);
    const line = container.querySelector(".w-px.bg-border");
    expect(line).toBeInTheDocument();
  });
});
