// src/features/marketplace/routes/InstallSteps.test.tsx
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { RECEIPTS } from "@/features/marketplace/data/fixtures/receipts";
import { InstallSteps } from "./InstallSteps";

const receipt = RECEIPTS["0xdemo-tx"];

function wrap(ui: React.ReactElement) {
  return render(<MemoryRouter>{ui}</MemoryRouter>);
}

describe("InstallSteps", () => {
  it("renders all four step titles", () => {
    wrap(<InstallSteps receipt={receipt} />);
    expect(screen.getByText(/XVN install detected/i)).toBeInTheDocument();
    expect(screen.getByText(/Decrypt sealed bundle/i)).toBeInTheDocument();
    expect(screen.getByText(/Install missing ingredients/i)).toBeInTheDocument();
    expect(screen.getByText(/Add to your Strategies/i)).toBeInTheDocument();
  });

  it("step 1 renders as done (struck-through) when xvnDetected is true", () => {
    wrap(<InstallSteps receipt={receipt} />);
    const step1title = screen.getByText(/XVN install detected/i);
    // done steps get line-through decoration
    expect(step1title.className).toMatch(/line-through/);
  });

  it("step 3 renders ingredient chips for all ingredients", () => {
    wrap(<InstallSteps receipt={receipt} />);
    for (const ing of receipt.install.ingredients) {
      expect(screen.getByText(ing.name)).toBeInTheDocument();
    }
  });

  it("installed ingredients show a different tone to missing ones", () => {
    wrap(<InstallSteps receipt={receipt} />);
    const installed = receipt.install.ingredients.filter((i) => i.installed);
    const missing   = receipt.install.ingredients.filter((i) => !i.installed);
    // Installed chips carry data-installed="true" for test accessibility
    expect(
      screen.getAllByTestId("ingredient-chip").filter(
        (el) => el.getAttribute("data-installed") === "true"
      )
    ).toHaveLength(installed.length);
    expect(
      screen.getAllByTestId("ingredient-chip").filter(
        (el) => el.getAttribute("data-installed") === "false"
      )
    ).toHaveLength(missing.length);
  });

  it("shows xvnEndpoint in step 1 description when detected", () => {
    wrap(<InstallSteps receipt={receipt} />);
    expect(screen.getByText(/localhost:3000/)).toBeInTheDocument();
  });

  it("shows 'not detected' message in step 1 when xvnDetected is false", () => {
    const noXvn = {
      ...receipt,
      install: { ...receipt.install, xvnDetected: false },
    };
    wrap(<InstallSteps receipt={noXvn} />);
    expect(screen.getByText(/not detected/i)).toBeInTheDocument();
  });

  it("step 3 action chip shows count of missing ingredients", () => {
    wrap(<InstallSteps receipt={receipt} />);
    const missingCount = receipt.install.ingredients.filter((i) => !i.installed).length;
    expect(screen.getByText(new RegExp(`Install missing \\(${missingCount}\\)`))).toBeInTheDocument();
  });
});
