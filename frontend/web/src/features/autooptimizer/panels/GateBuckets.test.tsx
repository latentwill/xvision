import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { GateBuckets } from "./GateBuckets";

describe("GateBuckets", () => {
  it("renders the three bucket counts and labels", () => {
    render(<GateBuckets kept={2} suspect={1} dropped={3} />);
    expect(screen.getByText("Anti-overfit gate")).toBeInTheDocument();
    expect(screen.getByText("Kept")).toBeInTheDocument();
    expect(screen.getByText("Suspect")).toBeInTheDocument();
    expect(screen.getByText("Dropped")).toBeInTheDocument();
    // counts
    expect(screen.getByText("2")).toBeInTheDocument();
    expect(screen.getByText("1")).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
  });
});
