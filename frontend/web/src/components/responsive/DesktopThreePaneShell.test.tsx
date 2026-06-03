import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ChatRailProps } from "@/components/shell/ChatRail";
import { useUi } from "@/stores/ui";
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
    localStorage.clear();
    useUi.setState({ chatRailOpen: false, sidebarWidth: 220, chatRailWidth: 380 });
  });

  afterEach(() => {
    localStorage.clear();
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

    // Shell grid uses inline style instead of a Tailwind grid-cols class so
    // column widths can be set dynamically from the store.
    const shell = container.firstElementChild as HTMLElement;
    expect(shell).toHaveClass("grid", "min-h-screen");
    expect(shell.style.gridTemplateColumns).toContain("220px");
    expect(shell.style.gridTemplateColumns).toContain("minmax(0,1fr)");

    // DOM order: Sidebar | ResizeHandle | main | [chat-rail wrapper] | …
    expect(shell.children[0]).toBe(sidebar);
    expect(shell.children[2]).toBe(main);
    expect(shell.children[3]).toContainElement(chatRail);

    // Regression guard for the text-overlap QA: the middle column must keep
    // its width hard-capped (max-w-[960px]) AND its track must be allowed to
    // shrink (minmax(0,1fr)). Together these stop unbreakable inner content
    // from pushing the main column past the chat rail. Centered via
    // justify-self so it doesn't drift left when the cap kicks in.
    expect(main).toHaveClass("min-w-0", "max-w-[960px]", "justify-self-center");
  });

  it("keeps the third grid cell mounted while the chat rail is suspended", () => {
    const neverResolves = new Promise<void>(() => {});
    function SuspendingRail(_: ChatRailProps): JSX.Element {
      throw neverResolves;
    }

    const { container } = render(
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route element={<DesktopThreePaneShell ChatRailComponent={SuspendingRail} />}>
            <Route index element={<div>Route content</div>} />
          </Route>
        </Routes>
      </MemoryRouter>,
    );

    const shell = container.firstElementChild;
    const main = screen.getByRole("main");

    // children[2] = main, children[3] = div wrapping the suspending rail
    expect(shell?.children[3]).toBeInTheDocument();
    expect(shell?.children[2]).toBe(main);
  });

  it("includes a second ResizeHandle and 5-column grid when the chat rail is open", () => {
    useUi.setState({ chatRailOpen: true, sidebarWidth: 220, chatRailWidth: 380 });

    const { container } = render(
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route element={<DesktopThreePaneShell ChatRailComponent={StubChatRail} />}>
            <Route index element={<div>Route content</div>} />
          </Route>
        </Routes>
      </MemoryRouter>,
    );

    const shell = container.firstElementChild as HTMLElement;
    // 5-column template when rail is open: sidebar 4px center 4px auto
    expect(shell.style.gridTemplateColumns).toMatch(/220px 4px minmax\(0,1fr\) 4px auto/);
  });
});
