import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { PhaseRibbon } from "./PhaseRibbon";
import type { Phase } from "../selectors/buildBoardState";

describe("PhaseRibbon", () => {
  it("renders the four phase labels", () => {
    renderWithProviders(<PhaseRibbon phase="idle" />);
    expect(screen.getByText("Propose")).toBeInTheDocument();
    expect(screen.getByText("Eval")).toBeInTheDocument();
    expect(screen.getByText("Gate")).toBeInTheDocument();
    expect(screen.getByText("Keep")).toBeInTheDocument();
  });

  it("marks nothing active when phase is idle", () => {
    renderWithProviders(<PhaseRibbon phase="idle" />);
    expect(screen.queryByRole("listitem", { current: "step" })).not.toBeInTheDocument();
    // Check no li has aria-current="step"
    const items = document.querySelectorAll("[aria-current='step']");
    expect(items.length).toBe(0);
  });

  it("marks Propose as active when phase is propose", () => {
    renderWithProviders(<PhaseRibbon phase="propose" />);
    const active = document.querySelectorAll("[aria-current='step']");
    expect(active.length).toBe(1);
    expect(active[0]).toHaveTextContent("Propose");
  });

  it("marks Eval as active when phase is eval", () => {
    renderWithProviders(<PhaseRibbon phase="eval" />);
    const active = document.querySelectorAll("[aria-current='step']");
    expect(active.length).toBe(1);
    expect(active[0]).toHaveTextContent("Eval");
    // Propose should be done
    const propose = screen.getByText("Propose").closest("li");
    expect(propose).not.toHaveAttribute("aria-current", "step");
  });

  it("marks Gate as active when phase is gate", () => {
    renderWithProviders(<PhaseRibbon phase="gate" />);
    const active = document.querySelectorAll("[aria-current='step']");
    expect(active.length).toBe(1);
    expect(active[0]).toHaveTextContent("Gate");
  });

  it("marks Keep as active when phase is keep", () => {
    renderWithProviders(<PhaseRibbon phase="keep" />);
    const active = document.querySelectorAll("[aria-current='step']");
    expect(active.length).toBe(1);
    expect(active[0]).toHaveTextContent("Keep");
  });

  it("marks all four phases done when phase is done", () => {
    renderWithProviders(<PhaseRibbon phase="done" />);
    // No active phase
    expect(document.querySelectorAll("[aria-current='step']").length).toBe(0);
    // All four phases should appear (still rendered), each with a ✓ prefix
    expect(screen.getByText("✓ Propose")).toBeInTheDocument();
    expect(screen.getByText("✓ Eval")).toBeInTheDocument();
    expect(screen.getByText("✓ Gate")).toBeInTheDocument();
    expect(screen.getByText("✓ Keep")).toBeInTheDocument();
  });

  it("phase=done renders the trailing 'Cycle complete' caption chip", () => {
    renderWithProviders(<PhaseRibbon phase="done" />);
    expect(screen.getByText("Cycle complete")).toBeInTheDocument();
  });

  it("non-done phases do not render the 'Cycle complete' chip or ✓ prefixes", () => {
    renderWithProviders(<PhaseRibbon phase="eval" />);
    expect(screen.queryByText("Cycle complete")).not.toBeInTheDocument();
    // Propose is past, but the ✓ prefix is reserved for the all-done state
    expect(screen.getByText("Propose")).toBeInTheDocument();
    expect(screen.queryByText(/✓/)).not.toBeInTheDocument();
  });

  it("phase=idle renders the 'No cycle running' caption", () => {
    renderWithProviders(<PhaseRibbon phase="idle" />);
    expect(screen.getByText("No cycle running")).toBeInTheDocument();
    expect(screen.queryByText("Cycle complete")).not.toBeInTheDocument();
  });

  it("non-idle phases do not render the 'No cycle running' caption", () => {
    renderWithProviders(<PhaseRibbon phase="done" />);
    expect(screen.queryByText("No cycle running")).not.toBeInTheDocument();
  });

  it("phase=done marks all phases as completed (not active)", () => {
    renderWithProviders(<PhaseRibbon phase="done" />);
    // Verify no aria-current, and each phase is accessible as a list item
    const items = screen.getAllByRole("listitem");
    expect(items.length).toBe(4);
    for (const item of items) {
      expect(item).not.toHaveAttribute("aria-current", "step");
      expect(item).toHaveTextContent("✓");
    }
  });

  it("renders as an ordered list with aria-label", () => {
    renderWithProviders(<PhaseRibbon phase="propose" />);
    const list = screen.getByRole("list", { name: /cycle phases/i });
    expect(list.tagName).toBe("OL");
  });

  it("each phase maps correctly — propose active does not mark eval active", () => {
    renderWithProviders(<PhaseRibbon phase="propose" />);
    const evalItem = screen.getByText("Eval").closest("li");
    expect(evalItem).not.toHaveAttribute("aria-current", "step");
    const gateItem = screen.getByText("Gate").closest("li");
    expect(gateItem).not.toHaveAttribute("aria-current", "step");
    const keepItem = screen.getByText("Keep").closest("li");
    expect(keepItem).not.toHaveAttribute("aria-current", "step");
  });

  it.each<[Phase, string]>([
    ["propose", "Propose"],
    ["eval", "Eval"],
    ["gate", "Gate"],
    ["keep", "Keep"],
  ])("phase=%s → only %s has aria-current=step", (phase, label) => {
    renderWithProviders(<PhaseRibbon phase={phase} />);
    const active = document.querySelectorAll("[aria-current='step']");
    expect(active.length).toBe(1);
    expect(active[0]).toHaveTextContent(label);
  });

  describe("running state", () => {
    it("running with no telemetry yet (phase=idle) shows a 'Starting' caption, not 'No cycle running'", () => {
      renderWithProviders(<PhaseRibbon phase="idle" running />);
      expect(screen.getByText(/starting/i)).toBeInTheDocument();
      expect(screen.queryByText("No cycle running")).not.toBeInTheDocument();
      expect(screen.queryByText("Cycle complete")).not.toBeInTheDocument();
    });

    it("running mid-cycle pulses the active step (and still marks it current)", () => {
      renderWithProviders(<PhaseRibbon phase="eval" running />);
      const active = document.querySelectorAll("[aria-current='step']");
      expect(active.length).toBe(1);
      expect(active[0]).toHaveTextContent("Eval");
      expect(active[0]?.className).toMatch(/animate-pulse/);
    });

    it("a frozen (paused) mid-cycle ribbon marks the step current WITHOUT pulsing", () => {
      renderWithProviders(<PhaseRibbon phase="eval" running={false} />);
      const active = document.querySelectorAll("[aria-current='step']");
      expect(active.length).toBe(1);
      expect(active[0]?.className).not.toMatch(/animate-pulse/);
      // Not finished, so no completion caption.
      expect(screen.queryByText("Cycle complete")).not.toBeInTheDocument();
    });

    it("running never renders the all-done ✓ / 'Cycle complete' chrome", () => {
      renderWithProviders(<PhaseRibbon phase="gate" running />);
      expect(screen.queryByText(/✓/)).not.toBeInTheDocument();
      expect(screen.queryByText("Cycle complete")).not.toBeInTheDocument();
    });
  });
});
