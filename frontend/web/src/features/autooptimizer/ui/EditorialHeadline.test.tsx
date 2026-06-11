import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import { EditorialHeadline } from "./EditorialHeadline";
import { renderWithProviders } from "../test-utils";

describe("EditorialHeadline", () => {
  it("renders headline, digest line, and action slot", () => {
    renderWithProviders(
      <EditorialHeadline
        headline={{ title: "Last ran 3h ago — kept 2 of 14 experiments.", subtitle: "5 active lineages." }}
        digest={{ experiments: 54, kept: 7, tokens: "31.8M", spend: "$15.57" }}
      >
        <button>Launch run</button>
      </EditorialHeadline>,
    );
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Last ran 3h ago");
    expect(screen.getByText(/54 experiments/)).toBeInTheDocument();
    expect(screen.getByText(/\$15\.57/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Launch run" })).toBeInTheDocument();
  });

  it("renders the subtitle text", () => {
    renderWithProviders(
      <EditorialHeadline
        headline={{ title: "A run is in progress.", subtitle: "1 cycle running · 3 active lineages." }}
        digest={{ experiments: 10, kept: 2, tokens: "5.1M", spend: "$3.20" }}
      />,
    );
    expect(screen.getByText(/1 cycle running/)).toBeInTheDocument();
  });

  it("omits the digest row when digest is null", () => {
    renderWithProviders(
      <EditorialHeadline
        headline={{ title: "The optimizer hasn't run yet.", subtitle: "Launch its first cycle." }}
        digest={null}
      />,
    );
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("The optimizer hasn't run yet.");
    expect(screen.queryByText(/experiments/)).not.toBeInTheDocument();
  });

  it("renders the kept count in the digest", () => {
    renderWithProviders(
      <EditorialHeadline
        headline={{ title: "Last ran 1d ago — kept 3 of 20 experiments.", subtitle: "2 active lineages." }}
        digest={{ experiments: 20, kept: 3, tokens: "10M", spend: "$5.00" }}
      />,
    );
    expect(screen.getByText(/3 kept/)).toBeInTheDocument();
    expect(screen.getByText(/10M tokens/)).toBeInTheDocument();
  });

  it("renders a 3-item digest gracefully when tokens are absent", () => {
    renderWithProviders(
      <EditorialHeadline
        headline={{ title: "A run is paused.", subtitle: "Resume it to keep experimenting." }}
        digest={{ experiments: 8, kept: 1, spend: "$2.10" }}
      />,
    );
    expect(screen.getByText(/8 experiments/)).toBeInTheDocument();
    expect(screen.getByText(/\$2\.10/)).toBeInTheDocument();
    // tokens segment should not appear
    expect(screen.queryByText(/tokens/)).not.toBeInTheDocument();
  });
});
