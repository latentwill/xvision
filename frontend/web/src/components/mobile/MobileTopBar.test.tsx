import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";

import { useUi } from "@/stores/ui";
import { MobileFunctionsSheet } from "./MobileFunctionsSheet";
import { MobileTopBar } from "./MobileTopBar";

// Regression guard for the iPhone safe-area-top fix (PR #181). The mobile
// header has to push its 52px content area below the notch / status bar
// using `env(safe-area-inset-top)`; without it, the menu button and title
// sit under the notch in standalone (Add-to-Home-Screen) and landscape.
// jsdom can't compute `env()` values, so we assert the class survives —
// removing it should fail this test.
describe("MobileTopBar", () => {
  it("applies safe-area-inset-top padding so header content clears the notch", () => {
    render(<MobileTopBar title="Strategies" onMenu={() => {}} />);
    const header = screen.getByRole("banner");
    const cls = header.className;
    expect(cls).toContain("pt-[env(safe-area-inset-top)]");
    expect(cls).toContain("h-[calc(52px+env(safe-area-inset-top))]");
  });

  it("combines side safe-area padding with the base horizontal gutter", () => {
    render(<MobileTopBar title="Strategies" onMenu={() => {}} />);
    const cls = screen.getByRole("banner").className;
    expect(cls).toContain("pl-[max(0.75rem,env(safe-area-inset-left))]");
    expect(cls).toContain("pr-[max(0.75rem,env(safe-area-inset-right))]");
    expect(cls).not.toContain("px-3");
  });
});

describe("MobileFunctionsSheet", () => {
  it("reserves bottom safe area for the action list", () => {
    useUi.setState({ mobileFunctionsOpen: true });

    render(
      <MemoryRouter>
        <MobileFunctionsSheet />
      </MemoryRouter>,
    );

    expect(screen.getByText("Create").parentElement?.parentElement).toHaveClass(
      "pb-[max(1.25rem,env(safe-area-inset-bottom))]",
    );
    useUi.setState({ mobileFunctionsOpen: false });
  });
});
