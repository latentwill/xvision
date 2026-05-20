import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { MarkdownView } from "./MarkdownView";

describe("MarkdownView", () => {
  it("renders external links without opener access", () => {
    render(<MarkdownView text="[open](https://attacker.example)" />);

    const link = screen.getByRole("link", { name: "open" });
    expect(link).toHaveAttribute("href", "https://attacker.example");
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noopener noreferrer");
  });

  it("does not render raw HTML or preserve dangerous link protocols", () => {
    const { container } = render(
      <MarkdownView
        text={'[bad](javascript:alert(1)) <script>alert("x")</script>'}
      />,
    );

    expect(screen.queryByRole("link", { name: "bad" })).toBeNull();
    expect(screen.getByText("bad")).toBeInTheDocument();
    expect(container.querySelector("script")).not.toBeInTheDocument();
  });

  it("renders fenced and inline code with code styling", () => {
    const { container } = render(
      <MarkdownView text={"Use `inline`.\n\n```ts\nconst value = 1;\n```"} />,
    );

    expect(screen.getByText("inline")).toHaveClass(
      "bg-surface-2/70",
      "font-mono",
    );
    expect(screen.getByText(/const value = 1/)).toHaveClass("font-mono");
    expect(container.querySelector("pre")).toHaveClass(
      "overflow-x-auto",
      "bg-surface-2/70",
    );
  });

  it("renders GFM tables with table structure and cell styling", () => {
    render(
      <MarkdownView
        text={"| Asset | Weight |\n| --- | ---: |\n| SPY | 60% |"}
      />,
    );

    expect(screen.getByRole("table")).toHaveClass("border-collapse");
    expect(screen.getByRole("columnheader", { name: "Asset" })).toHaveClass(
      "border",
      "font-medium",
    );
    expect(screen.getByRole("cell", { name: "SPY" })).toHaveClass("border");
  });

  it("renders paragraphs and ordered lists with expected structure", () => {
    const { container } = render(
      <MarkdownView text={"First paragraph.\n\n1. Review\n2. Ship"} />,
    );

    expect(screen.getByText("First paragraph.").tagName).toBe("P");
    expect(screen.getByRole("list")).toHaveClass("list-decimal");
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    expect(container.querySelector("p")).toHaveClass("my-1");
  });
});
