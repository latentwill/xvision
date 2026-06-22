import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { StrategyListItem } from "@/api/strategies";
import { StrategyPicker, strategySearchText } from "./StrategyPicker";

function strategy(overrides: Partial<StrategyListItem>): StrategyListItem {
  return {
    agent_id: "strat-alpha",
    display_name: "Alpha Breakout",
    template: "momentum",
    decision_cadence_minutes: 60,
    tags: ["btc", "trend"],
    bundle_hash: "hash-alpha",
    origin: "user",
    ...overrides,
  };
}

afterEach(() => cleanup());

describe("StrategyPicker", () => {
  it("builds search text from name, id, hash, tags, template, and origin", () => {
    const text = strategySearchText(strategy({}));

    expect(text).toContain("Alpha Breakout");
    expect(text).toContain("strat-alpha");
    expect(text).toContain("hash-alpha");
    expect(text).toContain("btc");
    expect(text).toContain("momentum");
    expect(text).toContain("user");
  });

  it("filters by stable id and selects the strategy", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();

    render(
      <StrategyPicker
        strategies={[
          strategy({ agent_id: "strat-alpha", display_name: "Alpha Breakout" }),
          strategy({
            agent_id: "strat-beta",
            display_name: "Beta Mean Reversion",
            bundle_hash: "bundle-beta",
          }),
        ]}
        value=""
        onChange={onChange}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Strategy" }));
    await user.type(screen.getByRole("textbox", { name: "Search Strategy" }), "bundle-beta");
    await user.click(screen.getByRole("option", { name: /Beta Mean Reversion/i }));

    expect(onChange).toHaveBeenCalledWith("strat-beta");
  });

  it("shows loading and no-strategies states", async () => {
    const user = userEvent.setup();
    const { rerender } = render(
      <StrategyPicker strategies={[]} value="" onChange={() => {}} loading />,
    );

    expect(screen.getByRole("button", { name: "Strategy" })).toHaveTextContent("Loading");

    rerender(<StrategyPicker strategies={[]} value="" onChange={() => {}} />);
    await user.click(screen.getByRole("button", { name: "Strategy" }));

    expect(screen.getByText("No strategies available")).toBeInTheDocument();
  });
});
