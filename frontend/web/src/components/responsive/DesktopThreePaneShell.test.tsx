import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

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

function renderShell() {
  return render(
    <MemoryRouter initialEntries={["/"]}>
      <Routes>
        <Route element={<DesktopThreePaneShell ChatRailComponent={StubChatRail} />}>
          <Route index element={<div>Route content</div>} />
        </Route>
      </Routes>
    </MemoryRouter>,
  );
}

describe("DesktopThreePaneShell", () => {
  beforeEach(() => {
    useFirstRunTourMock.mockClear();
    useUi.setState({ chatRailOpen: false });
  });

  it("renders the desktop shell panes and persistent shell UI", async () => {
    const { container } = renderShell();

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
    expect(shell?.children[2]).toContainElement(chatRail);
  });

  it("constrains center column width when chat rail is open", () => {
    useUi.setState({ chatRailOpen: true });
    renderShell();
    const main = screen.getByRole("main");
    // max-w cap + centering prevents unbreakable inner content from overflowing
    // into the chat rail column when the rail is visible.
    expect(main).toHaveClass("max-w-[960px]", "justify-self-center");
  });

  it("expands center column to fill available width when chat rail is closed", () => {
    useUi.setState({ chatRailOpen: false });
    renderShell();
    const main = screen.getByRole("main");
    expect(main).toHaveClass("min-w-0", "w-full");
    expect(main).not.toHaveClass("max-w-[960px]");
    expect(main).not.toHaveClass("justify-self-center");
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

    expect(shell?.children[2]).toBeInTheDocument();
    expect(shell?.children[1]).toBe(main);
  });
});
