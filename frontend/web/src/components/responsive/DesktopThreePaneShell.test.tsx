import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ChatRailProps } from "@/components/shell/ChatRail";
import { DesktopThreePaneShell } from "./DesktopThreePaneShell";

const useFirstRunTourMock = vi.hoisted(() => vi.fn());

vi.mock("@/components/shell/CommandPalette", () => ({
  CommandPalette: () => <div data-testid="command-palette" />,
}));

vi.mock("@/components/shell/Sidebar", () => ({
  Sidebar: () => <aside data-testid="sidebar" />,
}));

vi.mock("@/features/agent-runs/StripDockSlot", () => ({
  StripDockSlot: () => <div data-testid="strip-dock-slot" />,
}));

vi.mock("@/features/onboarding", () => ({
  useFirstRunTour: useFirstRunTourMock,
}));

function StubChatRail(_: ChatRailProps) {
  return <aside data-testid="chat-rail" />;
}

describe("DesktopThreePaneShell", () => {
  beforeEach(() => {
    useFirstRunTourMock.mockClear();
  });

  it("renders the desktop shell panes and persistent shell UI", async () => {
    const { container } = render(
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route element={<DesktopThreePaneShell ChatRailComponent={StubChatRail} />}>
            <Route index element={<div>Route content</div>} />
          </Route>
        </Routes>
      </MemoryRouter>,
    );

    expect(useFirstRunTourMock).toHaveBeenCalledTimes(1);
    const sidebar = screen.getByTestId("sidebar");
    const main = screen.getByRole("main");
    const chatRail = screen.getByTestId("chat-rail");

    expect(sidebar).toBeInTheDocument();
    expect(main).toHaveTextContent("Route content");
    expect(chatRail).toBeInTheDocument();
    expect(screen.getByTestId("command-palette")).toBeInTheDocument();
    expect(await screen.findByTestId("strip-dock-slot")).toBeInTheDocument();

    const shell = container.firstElementChild;
    expect(shell).toHaveClass("grid", "grid-cols-[220px_minmax(0,1fr)_auto]", "min-h-screen");
    expect(shell?.children[0]).toBe(sidebar);
    expect(shell?.children[1]).toBe(main);
    expect(shell?.children[2]).toBe(chatRail);

    // Regression guard for the text-overlap QA: the middle column must keep
    // its width hard-capped (max-w-[960px]) AND its track must be allowed to
    // shrink (minmax(0,1fr)). Together these stop unbreakable inner content
    // from pushing the main column past the chat rail. Centered via
    // justify-self so it doesn't drift left when the cap kicks in.
    expect(main).toHaveClass("min-w-0", "max-w-[960px]", "justify-self-center");
  });
});
