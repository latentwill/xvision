import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { GateBadge } from "./GateBadge";

describe("GateBadge", () => {
  it("renders the Kept label for an active node", () => {
    render(<GateBadge verdict="Accepted" status="active" />);
    expect(screen.getByText("Kept")).toBeInTheDocument();
  });
  it("renders Dropped for a rejected node", () => {
    render(<GateBadge verdict="Rejected" status="rejected" />);
    expect(screen.getByText("Dropped")).toBeInTheDocument();
  });
  it("renders Suspect for a quarantined node", () => {
    render(<GateBadge verdict="Suspect" status="quarantined" />);
    expect(screen.getByText("Suspect")).toBeInTheDocument();
  });
});
