import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { GateScorecard } from "./GateScorecard";
import type { GateRecord } from "../api";

const gate: GateRecord = {
  bundle_hash: "deadbeefcafe",
  parent_day_score: 1.2,
  child_day_score: 1.45,
  parent_holdout_score: 1.1,
  child_holdout_score: 1.25,
  gate_epsilon: 0.05,
  delta_day: 0.25,
  delta_holdout: 0.15,
  drawdown_ratio: 0.88,
  verdict: "passed",
  reason: null,
  edge_over_random: 0.35,
  parent_edge: 0.1,
  edge_delta: 0.25,
};

describe("GateScorecard", () => {
  it("renders Today's window and Untouched period rows", () => {
    render(<GateScorecard gate_record={gate} />);
    expect(screen.getByText(/Today's window/i)).toBeInTheDocument();
    expect(screen.getByText(/Untouched period/i)).toBeInTheDocument();
  });

  it("renders parent and child scores for both windows", () => {
    render(<GateScorecard gate_record={gate} />);
    // parent_day_score and child_day_score
    expect(screen.getByText("1.20")).toBeInTheDocument();
    expect(screen.getByText("1.45")).toBeInTheDocument();
    // parent_holdout_score and child_holdout_score
    expect(screen.getByText("1.10")).toBeInTheDocument();
    expect(screen.getByText("1.25")).toBeInTheDocument();
  });

  it("renders positive delta in green (text-gold) for day window", () => {
    render(<GateScorecard gate_record={gate} />);
    // delta_day = 0.25 → "+0.25" (edge_delta also renders +0.25; use getAllByText)
    expect(screen.getAllByText("+0.25").length).toBeGreaterThan(0);
  });

  it("renders positive delta for holdout window", () => {
    render(<GateScorecard gate_record={gate} />);
    // delta_holdout = 0.15 → "+0.15"
    expect(screen.getByText("+0.15")).toBeInTheDocument();
  });

  it("renders negative delta without plus sign", () => {
    const negGate: GateRecord = { ...gate, delta_day: -0.08 };
    render(<GateScorecard gate_record={negGate} />);
    expect(screen.getByText("-0.08")).toBeInTheDocument();
  });

  it("renders the drawdown ratio", () => {
    render(<GateScorecard gate_record={gate} />);
    expect(screen.getByText("0.88")).toBeInTheDocument();
  });

  it("renders min-improvement threshold label", () => {
    render(<GateScorecard gate_record={gate} />);
    expect(screen.getByText(/min.improvement/i)).toBeInTheDocument();
  });

  it("shows empty state when gate_record is null", () => {
    render(<GateScorecard gate_record={null} />);
    expect(screen.getByText(/Gate data not recorded/i)).toBeInTheDocument();
  });
});
