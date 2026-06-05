import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { EmptyPanel } from "./EmptyPanel";

describe("EmptyPanel", () => {
  it("renders the title and the phase hint", () => {
    render(<EmptyPanel title="Eval matrix" phase={2} hint="lights up when the regime matrix runs" />);
    expect(screen.getByText("Eval matrix")).toBeInTheDocument();
    expect(screen.getByText(/Phase 2/)).toBeInTheDocument();
    expect(screen.getByText(/regime matrix/)).toBeInTheDocument();
  });
});
