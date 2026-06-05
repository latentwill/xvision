import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { ExperimentPill } from "./ExperimentPill";

describe("ExperimentPill", () => {
  it("renders the kind label", () => {
    render(<ExperimentPill kind="Prompt tweak" />);
    expect(screen.getByText("Prompt tweak")).toBeInTheDocument();
  });
  it("defaults to Experiment", () => {
    render(<ExperimentPill />);
    expect(screen.getByText("Experiment")).toBeInTheDocument();
  });
});
