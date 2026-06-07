import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { PhaseStepper } from "./PhaseStepper";

const PHASE_NAMES = [
  "Briefing",
  "Parent selection",
  "Writing experiment",
  "Evaluating",
  "Gate review",
  "Committing",
  "Finishing",
];

describe("PhaseStepper", () => {
  it("renders exactly 7 phase chips", () => {
    renderWithProviders(<PhaseStepper currentPhase={null} completedPhases={[]} />);
    // All 7 phase chips must be visible
    for (const name of PHASE_NAMES) {
      expect(screen.getByText(name)).toBeInTheDocument();
    }
  });

  it("highlights the current phase chip", () => {
    renderWithProviders(
      <PhaseStepper currentPhase="Writing experiment" completedPhases={[]} />,
    );
    const chip = screen.getByText("Writing experiment").closest("[data-current]");
    expect(chip).not.toBeNull();
  });

  it("marks completed phases with a checkmark", () => {
    renderWithProviders(
      <PhaseStepper
        currentPhase="Evaluating"
        completedPhases={["Briefing", "Parent selection", "Writing experiment"]}
      />,
    );
    // completed chips have data-completed attribute
    const completed = document.querySelectorAll("[data-completed='true']");
    expect(completed.length).toBe(3);
  });

  it("shows all chips neutral when currentPhase is null", () => {
    renderWithProviders(<PhaseStepper currentPhase={null} completedPhases={[]} />);
    const current = document.querySelectorAll("[data-current='true']");
    expect(current.length).toBe(0);
  });
});
