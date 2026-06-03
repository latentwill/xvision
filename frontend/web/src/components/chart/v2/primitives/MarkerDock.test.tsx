import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { MarkerDock } from "./MarkerDock";
import type { V2Marker } from "../types";

const markers: V2Marker[] = [
  {
    kind: "buy",
    time: 1_700_000_000,
    price: 101,
    decision_index: 7,
    text: "Filled buy",
  },
  {
    kind: "veto",
    time: 1_700_003_600,
    price: 102,
    decision_index: 7,
    text: "Risk veto",
  },
];

describe("MarkerDock", () => {
  it("keeps marker identity scoped by kind when decision indexes overlap", () => {
    const onSelect = vi.fn();

    render(
      <MarkerDock markers={markers} activeId="veto:7" onSelect={onSelect} />,
    );

    const buyButton = screen.getByText("Filled buy").closest("button");
    const vetoButton = screen.getByText("Risk veto").closest("button");

    expect(buyButton).not.toHaveClass("bg-surface-elev");
    expect(vetoButton).toHaveClass("bg-surface-elev");

    fireEvent.click(vetoButton!);
    fireEvent.click(buyButton!);

    expect(onSelect).toHaveBeenNthCalledWith(1, "veto:7");
    expect(onSelect).toHaveBeenNthCalledWith(2, "buy:7");
  });
});
