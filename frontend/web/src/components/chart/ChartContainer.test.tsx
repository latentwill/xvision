import type { FormEvent } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ChartContainer } from "./ChartContainer";

describe("ChartContainer", () => {
  it("does not submit parent forms from toolbar controls", () => {
    const onSubmit = vi.fn((event: FormEvent<HTMLFormElement>) => {
      event.preventDefault();
    });

    render(
      <form onSubmit={onSubmit}>
        <ChartContainer
          title="Run chart"
          range="All"
          onRange={vi.fn()}
          layersPanel={<div>layers</div>}
          dataTable={<table><tbody><tr><td>row</td></tr></tbody></table>}
        >
          <div>chart</div>
        </ChartContainer>
      </form>,
    );

    fireEvent.click(screen.getByRole("button", { name: "1d" }));
    fireEvent.click(screen.getByRole("button", { name: /Layers/ }));
    fireEvent.click(screen.getByRole("button", { name: "Close" }));

    expect(onSubmit).not.toHaveBeenCalled();
  });
});
