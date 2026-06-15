// frontend/web/src/features/agent-runs/PullQuote.test.tsx
import { afterEach, describe, expect, test } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { PullQuote } from "./PullQuote";

// jsdom doesn't lay out, so scrollHeight/clientHeight are 0. Override them on
// the prototype to simulate content that does (or doesn't) overflow the clamp.
function mockMeasure(scrollHeight: number, clientHeight: number) {
  Object.defineProperty(HTMLElement.prototype, "scrollHeight", {
    configurable: true,
    get() {
      return scrollHeight;
    },
  });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    get() {
      return clientHeight;
    },
  });
}

afterEach(() => {
  cleanup();
  mockMeasure(0, 0); // restore the jsdom default
});

describe("PullQuote", () => {
  test("clamps a long body and toggles via SHOW MORE / SHOW LESS", () => {
    mockMeasure(200, 80); // full content taller than the clamp → overflows
    render(<PullQuote label="PROMPT" body={"line\n".repeat(40)} />);

    const body = screen.getByTestId("pullquote-body");
    expect(body.getAttribute("data-expanded")).toBe("false");

    const toggle = screen.getByTestId("pullquote-toggle");
    expect(toggle).toHaveTextContent(/show more/i);

    fireEvent.click(toggle);
    expect(screen.getByTestId("pullquote-body").getAttribute("data-expanded")).toBe("true");
    expect(screen.getByTestId("pullquote-toggle")).toHaveTextContent(/show less/i);
    // Expanded reveals the body in a scrollable box.
    expect(screen.getByTestId("pullquote-body").className).toMatch(/overflow-y-auto/);

    fireEvent.click(screen.getByTestId("pullquote-toggle"));
    expect(screen.getByTestId("pullquote-body").getAttribute("data-expanded")).toBe("false");
  });

  test("no toggle when the body fits within the clamp", () => {
    mockMeasure(50, 50); // fits → no overflow
    render(<PullQuote label="RESPONSE" body="short body" />);
    expect(screen.getByTestId("pullquote-body").getAttribute("data-expanded")).toBe("false");
    expect(screen.queryByTestId("pullquote-toggle")).toBeNull();
  });

  test("wraps a long unbroken line so it stays in the box", () => {
    mockMeasure(50, 50);
    render(<PullQuote label="PROMPT" body={"x".repeat(500)} />);
    const body = screen.getByTestId("pullquote-body");
    // pre-wrap preserves newlines; overflow-wrap:anywhere breaks a space-less line.
    expect(body.className).toMatch(/whitespace-pre-wrap/);
    expect(body.className).toMatch(/overflow-wrap:anywhere/);
  });

  test("a streaming body is never clamped (live tail stays visible)", () => {
    mockMeasure(200, 80);
    render(<PullQuote label="RESPONSE" body={"tok ".repeat(80)} streaming />);
    // The clamp wrapper is absent on the streaming path — the live body renders
    // in full so the operator sees the latest tokens.
    expect(screen.queryByTestId("pullquote-body")).toBeNull();
    expect(screen.queryByTestId("pullquote-toggle")).toBeNull();
  });
});
