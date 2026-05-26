import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { Card, CardHeader } from "./Card";

describe("CardHeader", () => {
  it("renders falsy but valid action content", () => {
    render(<CardHeader title="Risk" actions={0} />);

    expect(screen.getByText("0")).toBeInTheDocument();
  });

  it("constrains the title via min-w-0 + truncate so long headings don't push actions out", () => {
    // Regression: cards inside flex/grid tracks were overflowing when titles
    // were long because the h2 had no `min-w-0`. The action cluster gets
    // `shrink-0` so it stays put while the title truncates.
    render(<CardHeader title="A really really really long card title" actions={<span>x</span>} />);
    const heading = screen.getByRole("heading");
    expect(heading.className).toMatch(/min-w-0/);
    expect(heading.className).toMatch(/truncate/);
    const actions = screen.getByText("x").parentElement;
    expect(actions?.className).toMatch(/shrink-0/);
  });
});

describe("Card", () => {
  it("applies min-w-0 by default so its track can shrink without overlap", () => {
    // Regression: Card was missing min-w-0; long unbreakable inner content
    // (mono IDs, code blocks) was pushing the card past its grid track and
    // overlapping the next column. min-w-0 lets the track honour its width.
    const { container } = render(<Card>contents</Card>);
    const root = container.firstElementChild as HTMLElement;
    expect(root.className).toMatch(/min-w-0/);
  });
});
