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
});
