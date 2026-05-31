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

  it("renders data: and vbscript: hrefs as inert spans, not links", () => {
    render(
      <MemoryRouter>
        <ContentBlockView
          block={{
            type: "choice_chips",
            chips: [
              { label: "DataHtml", href: "data:text/html,<h1>xss</h1>" },
              { label: "Vbscript", href: "vbscript:msgbox(1)" },
            ],
          }}
        />
      </MemoryRouter>,
    );

    expect(screen.queryByRole("link", { name: "DataHtml" })).toBeNull();
    expect(screen.getByText("DataHtml").tagName).toBe("SPAN");
    expect(screen.queryByRole("link", { name: "Vbscript" })).toBeNull();
    expect(screen.getByText("Vbscript").tagName).toBe("SPAN");
  });

  it("renders JAVASCRIPT: (uppercase) hrefs as inert spans", () => {
    render(
      <MemoryRouter>
        <ContentBlockView
          block={{
            type: "choice_chips",
            chips: [{ label: "UpperJs", href: "JAVASCRIPT:alert(1)" }],
          }}
        />
      </MemoryRouter>,
    );

    expect(screen.queryByRole("link", { name: "UpperJs" })).toBeNull();
    expect(screen.getByText("UpperJs").tagName).toBe("SPAN");
  });

  it("renders javascript: hrefs as inert spans, safe hrefs as links", () => {
    render(
      <MemoryRouter>
        <ContentBlockView
          block={{
            type: "choice_chips",
            chips: [
              { label: "Bad", href: "javascript:alert(1)" },
              { label: "Good relative", href: "/safe-path" },
              { label: "Good https", href: "https://example.com" },
            ],
          }}
        />
      </MemoryRouter>,
    );

    expect(screen.queryByRole("link", { name: "Bad" })).toBeNull();
    expect(screen.getByText("Bad").tagName).toBe("SPAN");
    expect(screen.getByRole("link", { name: "Good relative" })).toHaveAttribute(
      "href",
      "/safe-path",
    );
    expect(screen.getByRole("link", { name: "Good https" })).toHaveAttribute(
      "href",
      "https://example.com",
    );
  });
});
