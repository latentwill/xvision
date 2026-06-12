import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { TrustFrameStrip } from "./TrustFrame";

afterEach(cleanup);

describe("TrustFrameStrip", () => {
  it("renders the honesty envelope from metrics", () => {
    render(
      <TrustFrameStrip
        metrics={{
          status: "completed",
          total_return_pct: 4.2,
          sharpe: 1.1,
          evidence_grade: "B",
          n_trades: 12,
          n_real_decisions: 9,
          n_synthesized_decisions: 2,
          insufficient_sample: false,
          annualization_calendar: "us_market_252x390m",
          return_ci_low: -1.2,
          return_ci_high: 7.4,
          sharpe_ci_low: 0.2,
          sharpe_ci_high: 1.8,
        }}
      />,
    );

    expect(screen.getByText("grade B")).toBeInTheDocument();
    expect(screen.getByText("decisions 9/2 real/synth")).toBeInTheDocument();
    expect(screen.getByText("calendar US session")).toBeInTheDocument();
    expect(screen.getByText(/CI -1\.20%..7\.40%/)).toBeInTheDocument();
    expect(screen.getByText(/CI 0\.20..1\.80/)).toBeInTheDocument();
  });

  it("marks failed runs instead of implying a trustworthy zero", () => {
    render(<TrustFrameStrip metrics={{ status: "failed", error: "boom" }} />);
    expect(screen.getByText("failed")).toBeInTheDocument();
    expect(screen.getByText("failed run")).toBeInTheDocument();
  });
});
