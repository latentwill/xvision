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
  it("keeps the rail grid cell mounted while the chat rail is suspended", () => {
    const { container } = render(
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route element={<TabletSplitShell ChatRailComponent={SuspendedChatRail} />}>
            <Route index element={<div>Route content</div>} />
          </Route>
        </Routes>
      </MemoryRouter>,
    );

    const shell = container.firstElementChild;
    const main = screen.getByRole("main");

    expect(shell?.children[0]).not.toBe(main);
    expect(shell?.children[1]).toBe(main);
    expect(shell?.children[0]).toHaveClass("min-w-0", "overflow-hidden");
  });
});
