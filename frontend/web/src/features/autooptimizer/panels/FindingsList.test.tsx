import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FindingsList } from "./FindingsList";
import type { ExperimentFinding } from "../api";

const infoFinding: ExperimentFinding = {
  id: 1,
  bundle_hash: "deadbeef",
  severity: "info",
  code: "INFO_001",
  summary: "Diversity score is within normal range",
  detail: null,
  model: "gpt-4o",
};

const warnFinding: ExperimentFinding = {
  id: 2,
  bundle_hash: "deadbeef",
  severity: "warn",
  code: "WARN_002",
  summary: "Sharpe improvement narrow",
  detail: "The improvement is within the epsilon band",
  model: "claude-3-5-sonnet",
};

const riskFinding: ExperimentFinding = {
  id: 3,
  bundle_hash: "deadbeef",
  severity: "risk",
  code: "RISK_003",
  summary: "High drawdown detected",
  detail: "Max drawdown exceeds the 15% threshold set in the gate configuration",
  model: null,
};

describe("FindingsList", () => {
  it("renders info severity badge as blue", () => {
    render(<FindingsList findings={[infoFinding]} />);
    const badge = screen.getByText("info");
    expect(badge).toBeInTheDocument();
    // Badge should have blue styling class
    expect(badge.className).toMatch(/blue/i);
  });

  it("renders warn severity badge as amber", () => {
    render(<FindingsList findings={[warnFinding]} />);
    const badge = screen.getByText("warn");
    expect(badge).toBeInTheDocument();
    expect(badge.className).toMatch(/amber|warn/i);
  });

  it("renders risk severity badge as red", () => {
    render(<FindingsList findings={[riskFinding]} />);
    const badge = screen.getByText("risk");
    expect(badge).toBeInTheDocument();
    expect(badge.className).toMatch(/red|danger/i);
  });

  it("renders code and summary as title", () => {
    render(<FindingsList findings={[infoFinding]} />);
    expect(screen.getByText("INFO_001")).toBeInTheDocument();
    expect(screen.getByText("Diversity score is within normal range")).toBeInTheDocument();
  });

  it("renders model label when present", () => {
    render(<FindingsList findings={[infoFinding]} />);
    expect(screen.getByText("gpt-4o")).toBeInTheDocument();
  });

  it("does not render model label when null", () => {
    render(<FindingsList findings={[riskFinding]} />);
    // model is null, no model text should appear
    expect(screen.queryByText("null")).not.toBeInTheDocument();
  });

  it("renders multiple findings", () => {
    render(<FindingsList findings={[infoFinding, warnFinding, riskFinding]} />);
    expect(screen.getByText("INFO_001")).toBeInTheDocument();
    expect(screen.getByText("WARN_002")).toBeInTheDocument();
    expect(screen.getByText("RISK_003")).toBeInTheDocument();
  });

  it("shows empty state when findings is empty", () => {
    render(<FindingsList findings={[]} />);
    expect(screen.getByText(/No reviewer notes for this experiment/i)).toBeInTheDocument();
  });

  it("shows detail text for finding with detail", () => {
    render(<FindingsList findings={[warnFinding]} />);
    expect(screen.getByText("The improvement is within the epsilon band")).toBeInTheDocument();
  });

  it("shows collapse/expand toggle for long detail text", async () => {
    const user = userEvent.setup();
    const longDetail =
      "This is a very long detail text that should be truncated. ".repeat(10);
    const finding: ExperimentFinding = {
      ...riskFinding,
      detail: longDetail,
    };
    render(<FindingsList findings={[finding]} />);
    // Should have a toggle button (Show more / Show less)
    const toggle = screen.getByRole("button", { name: /show more|expand/i });
    expect(toggle).toBeInTheDocument();
    await user.click(toggle);
    // After clicking, toggle text changes to "Show less" or "Collapse"
    expect(screen.getByRole("button", { name: /show less|collapse/i })).toBeInTheDocument();
  });
});
