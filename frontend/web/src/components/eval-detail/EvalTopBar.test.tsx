import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";

import { EvalTopBar } from "./EvalTopBar";

describe("EvalTopBar", () => {
  it("does not label unknown statuses as completed", () => {
    render(
      <MemoryRouter>
        <EvalTopBar runId="run1" status="timed_out" />
      </MemoryRouter>,
    );

    expect(screen.getByText("EVAL TIMED_OUT")).toBeInTheDocument();
    expect(screen.queryByText("EVAL COMPLETED")).toBeNull();
  });
});
