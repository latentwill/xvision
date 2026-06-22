import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SignalSearchableSelectMenu } from "./SignalMenu";

const OPTIONS = [
  {
    value: "strat-alpha",
    label: "Alpha Breakout",
    meta: "strat-alpha",
    searchText: "Alpha Breakout strat-alpha hash-a",
  },
  {
    value: "strat-beta",
    label: "Beta Mean Reversion",
    meta: "strat-beta",
    searchText: "Beta Mean Reversion strat-beta hash-b",
  },
];

afterEach(() => cleanup());

describe("SignalSearchableSelectMenu", () => {
  it("filters options by search text and selects the match", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();

    render(
      <SignalSearchableSelectMenu
        ariaLabel="Strategy"
        value=""
        options={OPTIONS}
        onChange={onChange}
        placeholder="Pick strategy"
        searchPlaceholder="Search strategies…"
      />,
    );

    await user.click(screen.getByRole("button", { name: "Strategy" }));
    await user.type(screen.getByRole("textbox", { name: "Search Strategy" }), "beta");

    expect(screen.queryByRole("option", { name: /Alpha Breakout/i })).not.toBeInTheDocument();
    await user.click(screen.getByRole("option", { name: /Beta Mean Reversion/i }));

    expect(onChange).toHaveBeenCalledWith("strat-beta");
  });

  it("selects the highlighted option with ArrowDown and Enter", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();

    render(
      <SignalSearchableSelectMenu
        ariaLabel="Strategy"
        value=""
        options={OPTIONS}
        onChange={onChange}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Strategy" }));
    await user.keyboard("{ArrowDown}{Enter}");

    expect(onChange).toHaveBeenCalledWith("strat-alpha");
  });

  it("shows no-results copy without rendering stale options", async () => {
    const user = userEvent.setup();

    render(
      <SignalSearchableSelectMenu
        ariaLabel="Strategy"
        value=""
        options={OPTIONS}
        onChange={() => {}}
        emptyHint="No strategies match"
      />,
    );

    await user.click(screen.getByRole("button", { name: "Strategy" }));
    await user.type(screen.getByRole("textbox", { name: "Search Strategy" }), "zzzz");

    expect(screen.getByText("No strategies match")).toBeInTheDocument();
    expect(screen.queryByRole("option")).not.toBeInTheDocument();
  });

  it("does not select an option when Enter fires on the closed trigger", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();

    render(
      <SignalSearchableSelectMenu
        ariaLabel="Strategy"
        value=""
        options={OPTIONS}
        onChange={onChange}
      />,
    );

    screen.getByRole("button", { name: "Strategy" }).focus();
    await user.keyboard("{Enter}");

    expect(onChange).not.toHaveBeenCalled();
  });

  it("exposes expanded state and listbox relationship", async () => {
    const user = userEvent.setup();

    render(
      <SignalSearchableSelectMenu
        ariaLabel="Strategy"
        value=""
        options={OPTIONS}
        onChange={() => {}}
      />,
    );

    const trigger = screen.getByRole("button", { name: "Strategy" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");

    await user.click(trigger);

    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("listbox", { name: "Strategy options" })).toBeInTheDocument();
    expect(within(screen.getByRole("listbox", { name: "Strategy options" })).getAllByRole("option")).toHaveLength(2);
  });
});
