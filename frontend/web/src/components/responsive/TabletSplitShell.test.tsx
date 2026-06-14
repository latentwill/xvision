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
    // Rail cell is the LAST child (right column) and stays mounted.
    expect(shell?.children[1]).not.toBe(main);
    expect(shell?.children[1]).toHaveClass("min-w-0", "overflow-hidden");
  });

  it("places main first and the chat rail on the right (QA #5)", () => {
    // The chat rail must stay on the right edge at tablet width, matching the
    // desktop three-pane shell — it must NOT flip to the left column.
    const shell = renderShell();
    const main = screen.getByRole("main");

    expect(shell?.children[0]).toBe(main);
    expect(shell?.children[1]).not.toBe(main);
    expect(shell).toHaveClass("grid-cols-[minmax(0,1fr)_min(360px,45vw)]");
  });
});
