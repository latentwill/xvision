import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import type { MutatorScore } from "../api";
import { WriterLadderChart } from "./WriterLadderChart";

const FIXTURE: MutatorScore[] = [
  {
    provider: "anthropic",
    model: "claude-haiku-4-5",
    prompt_version: "v1",
    proposals: 10,
    accepted: 6,
    rejected_overfit: 4,
    avg_delta_sharpe: 0.18,
  },
  {
    provider: "google",
    model: "gemini-2.5-flash-preview-04-17-thinking-experimental",
    prompt_version: "v1",
    proposals: 8,
    accepted: 3,
    rejected_overfit: 5,
    avg_delta_sharpe: -0.07,
  },
];

describe("WriterLadderChart — horizontal bar rows", () => {
  it("renders one row per MutatorScore", () => {
    render(<WriterLadderChart rows={FIXTURE} />);
    const rows = screen.getAllByTestId("writer-ladder-row");
    expect(rows.length).toBe(2);
  });

  it("shows the empty-state message when rows is empty", () => {
    render(<WriterLadderChart rows={[]} />);
    expect(screen.getByText(/no writer data yet/i)).toBeInTheDocument();
  });

  it("exposes the full provider/model name in a title attribute", () => {
    render(<WriterLadderChart rows={FIXTURE} />);
    expect(
      screen.getByTitle(
        "google/gemini-2.5-flash-preview-04-17-thinking-experimental",
      ),
    ).toBeInTheDocument();
    expect(screen.getByTitle("anthropic/claude-haiku-4-5")).toBeInTheDocument();
  });

  it("sets the accept-rate bar width from accepted/proposals", () => {
    render(<WriterLadderChart rows={FIXTURE} />);
    const bars = screen.getAllByTestId("accept-rate-bar");
    expect(bars[0]).toHaveStyle({ width: "60%" });
    expect(bars[1]).toHaveStyle({ width: "37.5%" });
  });

  it("renders accepted/proposals counts per row", () => {
    render(<WriterLadderChart rows={FIXTURE} />);
    expect(screen.getByText("6/10")).toBeInTheDocument();
    expect(screen.getByText("3/8")).toBeInTheDocument();
  });

  it("colors negative avg ΔSharpe with the danger class and positive with gold", () => {
    render(<WriterLadderChart rows={FIXTURE} />);
    const positive = screen.getByText(/\+0\.18/);
    expect(positive.className).toMatch(/text-gold/);
    const negative = screen.getByText(/0\.07/);
    expect(negative.className).toMatch(/text-danger/);
  });

  it("handles zero proposals without NaN width", () => {
    render(
      <WriterLadderChart
        rows={[
          {
            provider: "openai",
            model: "gpt-4o-mini",
            prompt_version: "v1",
            proposals: 0,
            accepted: 0,
            rejected_overfit: 0,
            avg_delta_sharpe: 0,
          },
        ]}
      />,
    );
    expect(screen.getByTestId("accept-rate-bar")).toHaveStyle({ width: "0%" });
  });
});
