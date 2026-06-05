import { describe, expect, it } from "vitest";
import { renderWithProviders } from "../test-utils";
import { screen } from "@testing-library/react";
import { Breadcrumb } from "./Breadcrumb";

describe("Breadcrumb", () => {
  it("renders crumbs with the last as current", () => {
    renderWithProviders(
      <Breadcrumb
        items={[
          { label: "OPTIMIZER", to: "/optimizer" },
          { label: "cycle" },
          { label: "cyc-1" },
        ]}
      />,
    );
    expect(screen.getByText("OPTIMIZER").closest("a")).toHaveAttribute("href", "/optimizer");
    expect(screen.getByText("cyc-1")).toBeInTheDocument();
    // Last crumb must carry aria-current="page"
    expect(screen.getByText("cyc-1")).toHaveAttribute("aria-current", "page");
    // A non-last linked crumb must NOT carry aria-current
    const optimizerEl = screen.getByText("OPTIMIZER");
    expect(optimizerEl).not.toHaveAttribute("aria-current");
  });
});
