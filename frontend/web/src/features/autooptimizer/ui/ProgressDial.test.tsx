import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { ProgressDial } from "./ProgressDial";

describe("ProgressDial", () => {
  it("shows the rounded percentage and clamps to 0..1", () => {
    render(<ProgressDial value={0.42} label="CYCLE" />);
    expect(screen.getByText("42%")).toBeInTheDocument();
    expect(screen.getByText("CYCLE")).toBeInTheDocument();
  });
  it("clamps out-of-range values", () => {
    render(<ProgressDial value={1.8} />);
    expect(screen.getByText("100%")).toBeInTheDocument();
  });
});
