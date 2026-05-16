import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";

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
});
