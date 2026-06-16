import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";

import type { ChatRailProps } from "@/components/shell/ChatRail";
import { TabletSplitShell } from "./TabletSplitShell";

vi.mock("@/components/shell/CommandPalette", () => ({
  CommandPalette: () => null,
}));

vi.mock("@/features/agent-runs/StripDockSlot", () => ({
  StripDockSlot: () => null,
}));

function SuspendedChatRail(_: ChatRailProps) {
  throw new Promise(() => {});
  return null;
}

describe("TabletSplitShell", () => {
  function renderShell() {
    const { container } = render(
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route element={<TabletSplitShell ChatRailComponent={SuspendedChatRail} />}>
            <Route index element={<div>Route content</div>} />
          </Route>
        </Routes>
      </MemoryRouter>,
    );
    return container.firstElementChild;
  }

  it("keeps the rail grid cell mounted while the chat rail is suspended", () => {
    const shell = renderShell();
    const main = screen.getByRole("main");
    // Rail cell is the LAST child (right column) and stays mounted even while
    // the chat rail component is suspended. Layout is: sidebar | main | rail.
    const railCell = shell?.children[2];
    expect(railCell).not.toBe(main);
    expect(railCell).toHaveClass("min-w-0", "overflow-hidden");
  });

  it("renders nav left, main middle, chat rail right (QA: side menu stays visible)", () => {
    // QA: the left nav must remain reachable at tablet width (it previously
    // disappeared), while QA #5's chat rail stays pinned to the RIGHT edge.
    const shell = renderShell();
    const main = screen.getByRole("main");

    // Left column is the compact sidebar nav, not main.
    expect(shell?.children[0]).not.toBe(main);
    expect(shell?.children[0]?.tagName).toBe("ASIDE");
    expect(screen.getByRole("navigation")).toBeInTheDocument();
    // Main is the middle column; rail is the last (right) column.
    expect(shell?.children[1]).toBe(main);
    expect(shell).toHaveClass("grid-cols-[60px_minmax(0,1fr)_min(320px,40vw)]");
  });
});
