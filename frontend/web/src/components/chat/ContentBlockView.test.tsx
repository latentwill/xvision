import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MemoryRouter } from "react-router-dom";

import { useUi } from "@/stores/ui";
import { ContentBlockView } from "./ContentBlockView";

afterEach(() => {
  useUi.setState({ cmdkOpen: false });
});

describe("ContentBlockView choice chips", () => {
  it("renders href chips as links and command chips as actions", async () => {
    render(
      <MemoryRouter>
        <ContentBlockView
          block={{
            type: "choice_chips",
            chips: [
              { label: "Open runs", href: "/eval-runs" },
              { label: "Search commands", command: "open_command_palette" },
              { label: "Static" },
            ],
          }}
        />
      </MemoryRouter>,
    );

    expect(screen.getByRole("link", { name: "Open runs" })).toHaveAttribute(
      "href",
      "/eval-runs",
    );

    await userEvent.click(
      screen.getByRole("button", { name: "Search commands" }),
    );
    expect(useUi.getState().cmdkOpen).toBe(true);
    expect(screen.getByText("Static").tagName).toBe("SPAN");
  });
});
