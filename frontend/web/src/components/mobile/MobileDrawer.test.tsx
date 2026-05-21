import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MemoryRouter } from "react-router-dom";

import { useUi } from "@/stores/ui";
import { MobileDrawer } from "./MobileDrawer";

function renderDrawer() {
  return render(
    <MemoryRouter>
      <button type="button">Open navigation</button>
      <MobileDrawer />
    </MemoryRouter>,
  );
}

afterEach(() => {
  act(() => {
    useUi.setState({ mobileDrawerOpen: false });
  });
});

describe("MobileDrawer", () => {
  it("acts as a modal dialog and restores focus on close", async () => {
    const user = userEvent.setup();
    renderDrawer();

    const opener = screen.getByRole("button", { name: "Open navigation" });
    opener.focus();
    act(() => {
      useUi.setState({ mobileDrawerOpen: true });
    });

    await waitFor(() => {
      expect(screen.getByRole("dialog", { name: "Navigation" })).toHaveAttribute(
        "aria-modal",
        "true",
      );
    });
    await waitFor(() => {
      expect(screen.getAllByRole("button", { name: "Close navigation" })[1])
        .toHaveFocus();
    });

    await user.keyboard("{Escape}");
    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: "Navigation" })).toBeNull();
    });
    expect(opener).toHaveFocus();
  });

  it("traps tab focus within the drawer", async () => {
    const user = userEvent.setup();
    renderDrawer();
    act(() => {
      useUi.setState({ mobileDrawerOpen: true });
    });

    await waitFor(() => {
      expect(screen.getAllByRole("button", { name: "Close navigation" }))
        .toHaveLength(2);
    });
    const close = screen.getAllByRole("button", { name: "Close navigation" })[1];
    await waitFor(() => expect(close).toHaveFocus());

    await user.keyboard("{Shift>}{Tab}{/Shift}");
    expect(screen.getByRole("button", { name: "View history" })).toHaveFocus();

    await user.tab();
    expect(close).toHaveFocus();
  });
});
