import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { DeltaCell } from "./DeltaCell";

describe("DeltaCell", () => {
  it("renders a positive delta with a sign", () => {
    render(<DeltaCell state="done" delta={0.22} sharpe={1.4} />);
    expect(screen.getByText("+0.22")).toBeInTheDocument();
  });
  it("renders a negative delta", () => {
    render(<DeltaCell state="done" delta={-0.13} sharpe={0.2} />);
    expect(screen.getByText("-0.13")).toBeInTheDocument();
  });
  it("renders state labels for non-done cells", () => {
    render(<DeltaCell state="running" />);
    expect(screen.getByText(/run/i)).toBeInTheDocument();
  });
  it("renders a queued state", () => {
    render(<DeltaCell state="queued" />);
    expect(screen.getByText(/queued|—/i)).toBeInTheDocument();
  });
  it("does not render NaN for a non-finite delta in a done cell", () => {
    render(<DeltaCell state="done" delta={NaN} />);
    expect(screen.queryByText(/NaN/)).toBeNull();
  });
  it("renders zero delta as +0.00 with gold tint", () => {
    render(<DeltaCell state="done" delta={0} />);
    expect(screen.getByText("+0.00")).toBeInTheDocument();
  });
});
