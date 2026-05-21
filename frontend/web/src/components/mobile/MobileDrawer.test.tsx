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
  it("renders as a non-modal nav landmark when open", async () => {
    renderDrawer();

    expect(screen.queryByRole("navigation", { name: "Navigation" })).toBeNull();

    act(() => {
      useUi.setState({ mobileDrawerOpen: true });
    });

    const nav = await screen.findByRole("navigation", { name: "Navigation" });
    expect(nav).not.toHaveAttribute("role", "dialog");
    expect(nav).not.toHaveAttribute("aria-modal");
  });

  it("closes via the Close navigation button and is removed from the DOM", async () => {
    const user = userEvent.setup();
    renderDrawer();
    act(() => {
      useUi.setState({ mobileDrawerOpen: true });
    });

    const close = await screen.findByRole("button", { name: "Close navigation" });
    await user.click(close);

    await waitFor(() => {
      expect(screen.queryByRole("navigation", { name: "Navigation" })).toBeNull();
    });
  });

  it("does not steal focus when it opens", async () => {
    renderDrawer();

    const opener = screen.getByRole("button", { name: "Open navigation" });
    opener.focus();
    expect(opener).toHaveFocus();

    act(() => {
      useUi.setState({ mobileDrawerOpen: true });
    });

    await screen.findByRole("navigation", { name: "Navigation" });
    expect(opener).toHaveFocus();
  });

  it("does not render a backdrop element", async () => {
    renderDrawer();
    act(() => {
      useUi.setState({ mobileDrawerOpen: true });
    });

    await screen.findByRole("navigation", { name: "Navigation" });
    const closeButtons = screen.getAllByRole("button", {
      name: "Close navigation",
    });
    expect(closeButtons).toHaveLength(1);
  });
});
