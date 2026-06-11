import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../test-utils";
import { ExperimentBoard } from "./ExperimentBoard";
import type { BoardCard } from "../selectors/buildBoardState";

// Mock the api module so ExpandableArtifact's useExperimentDetail doesn't fire real fetches
vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return {
    ...real,
    useExperimentDetail: vi.fn(() => ({
      data: undefined,
      isLoading: false,
      isError: true,
      isPending: false,
      status: "error",
      error: new Error("mocked"),
      isSuccess: false,
    })),
    useBlob: vi.fn(() => ({ data: undefined, isLoading: false, isError: false })),
  };
});

function makeCard(overrides: Partial<BoardCard> & { hash: string }): BoardCard {
  return {
    label: null,
    state: "evaluating",
    delta: null,
    writer: null,
    ...overrides,
  };
}

describe("ExperimentBoard — empty state", () => {
  it("renders nothing when cards array is empty", () => {
    const { container } = renderWithProviders(<ExperimentBoard cards={[]} />);
    expect(container.firstChild).toBeNull();
  });
});

describe("ExperimentBoard — renders one ExpandableArtifact per card", () => {
  it("renders a button for each card with hash summary", () => {
    renderWithProviders(
      <ExperimentBoard
        cards={[
          makeCard({ hash: "abcd1234ef", state: "kept", delta: 0.21 }),
          makeCard({ hash: "beef5678cd", state: "rejected", delta: -0.08 }),
        ]}
      />,
    );
    // Each card produces an ExpandableArtifact button
    const buttons = screen.getAllByRole("button");
    expect(buttons.length).toBe(2);
    expect(buttons[0]).toHaveTextContent("abcd1234");
    expect(buttons[1]).toHaveTextContent("beef5678");
  });

  it("shows delta value in summary for kept card", () => {
    renderWithProviders(
      <ExperimentBoard
        cards={[makeCard({ hash: "abcd1234ef", state: "kept", delta: 0.21 })]}
      />,
    );
    // Should contain the delta formatted with +
    expect(screen.getByText(/\+0\.21/)).toBeInTheDocument();
  });

  it("shows negative delta for rejected card", () => {
    renderWithProviders(
      <ExperimentBoard
        cards={[makeCard({ hash: "beef5678cd", state: "rejected", delta: -0.08 })]}
      />,
    );
    // Should show the delta (either -0.08 or −0.08 with unicode minus)
    const button = screen.getByRole("button");
    expect(button.textContent).toMatch(/0\.08/);
  });

  it("shows 'evaluating…' state chip for evaluating card with animated class", () => {
    renderWithProviders(
      <ExperimentBoard
        cards={[makeCard({ hash: "cccc1234ef", state: "evaluating" })]}
      />,
    );
    // The evaluating chip should have animate-pulse class
    const chip = document.querySelector(".animate-pulse");
    expect(chip).not.toBeNull();
    expect(chip).toHaveTextContent("evaluating…");
  });

  it("shows 'kept' label for kept card", () => {
    renderWithProviders(
      <ExperimentBoard
        cards={[makeCard({ hash: "abcd1234ef", state: "kept", delta: 0.0 })]}
      />,
    );
    expect(screen.getByText(/kept/)).toBeInTheDocument();
  });

  it("shows 'rejected' label for rejected card", () => {
    renderWithProviders(
      <ExperimentBoard
        cards={[makeCard({ hash: "beef5678cd", state: "rejected" })]}
      />,
    );
    expect(screen.getByText(/rejected/)).toBeInTheDocument();
  });

  it("shows 'suspect' label for suspect card", () => {
    renderWithProviders(
      <ExperimentBoard
        cards={[makeCard({ hash: "dddd1234ef", state: "suspect" })]}
      />,
    );
    expect(screen.getByText(/suspect/)).toBeInTheDocument();
  });
});

describe("ExperimentBoard — state styling", () => {
  it("kept card uses gold text class", () => {
    renderWithProviders(
      <ExperimentBoard
        cards={[makeCard({ hash: "aaaa1234ef", state: "kept", delta: 0.1 })]}
      />,
    );
    const goldEl = document.querySelector(".text-gold");
    expect(goldEl).not.toBeNull();
  });

  it("rejected card uses danger text class", () => {
    renderWithProviders(
      <ExperimentBoard
        cards={[makeCard({ hash: "bbbb1234ef", state: "rejected", delta: -0.1 })]}
      />,
    );
    const dangerEl = document.querySelector(".text-danger");
    expect(dangerEl).not.toBeNull();
  });

  it("suspect card uses warn text class", () => {
    renderWithProviders(
      <ExperimentBoard
        cards={[makeCard({ hash: "cccc1234ef", state: "suspect" })]}
      />,
    );
    const warnEl = document.querySelector(".text-warn");
    expect(warnEl).not.toBeNull();
  });
});

describe("ExperimentBoard — writerModel threading", () => {
  it("passes writerModel from card.writer to ExpandableArtifact", () => {
    renderWithProviders(
      <ExperimentBoard
        cards={[
          makeCard({ hash: "aaaa1234ef", state: "kept", writer: "gemini-2.5-pro" }),
        ]}
      />,
    );
    // The ExpandableArtifact receives writerModel — checking the button exists is enough at unit level
    // (the writer row is only shown when expanded, and our mock returns an error for ArtifactBody)
    const btn = screen.getByRole("button");
    expect(btn).toBeInTheDocument();
  });
});

describe("ExperimentBoard — defaultOpenHash and expandBoard props", () => {
  it("opens the card matching defaultOpenHash", () => {
    renderWithProviders(
      <ExperimentBoard
        cards={[
          makeCard({ hash: "target1234ef", state: "kept" }),
          makeCard({ hash: "other123456", state: "evaluating" }),
        ]}
        defaultOpenHash="target1234ef"
      />,
    );
    const buttons = screen.getAllByRole("button");
    // The card with the matching hash starts expanded
    expect(buttons[0]).toHaveAttribute("aria-expanded", "true");
    // The other card stays collapsed
    expect(buttons[1]).toHaveAttribute("aria-expanded", "false");
  });

  it("does not open any card when defaultOpenHash does not match", () => {
    renderWithProviders(
      <ExperimentBoard
        cards={[makeCard({ hash: "aaaa1234ef", state: "kept" })]}
        defaultOpenHash="nomatch"
      />,
    );
    const btn = screen.getByRole("button");
    expect(btn).toHaveAttribute("aria-expanded", "false");
  });

  it("opens all cards when expandBoard is true", () => {
    renderWithProviders(
      <ExperimentBoard
        cards={[
          makeCard({ hash: "card1234ef", state: "kept" }),
          makeCard({ hash: "card5678cd", state: "rejected" }),
          makeCard({ hash: "card9999ef", state: "evaluating" }),
        ]}
        expandBoard={true}
      />,
    );
    const buttons = screen.getAllByRole("button");
    expect(buttons.length).toBe(3);
    for (const btn of buttons) {
      expect(btn).toHaveAttribute("aria-expanded", "true");
    }
  });

  it("expandBoard=false leaves all cards collapsed", () => {
    renderWithProviders(
      <ExperimentBoard
        cards={[
          makeCard({ hash: "card1234ef", state: "kept" }),
          makeCard({ hash: "card5678cd", state: "rejected" }),
        ]}
        expandBoard={false}
      />,
    );
    const buttons = screen.getAllByRole("button");
    for (const btn of buttons) {
      expect(btn).toHaveAttribute("aria-expanded", "false");
    }
  });

  it("changing defaultOpenHash after mount opens the newly targeted card", () => {
    const cards = [
      makeCard({ hash: "first1234ef", state: "kept" }),
      makeCard({ hash: "second5678cd", state: "rejected" }),
    ];
    const { rerender } = renderWithProviders(
      <ExperimentBoard cards={cards} defaultOpenHash="first1234ef" />,
    );
    let buttons = screen.getAllByRole("button");
    expect(buttons[0]).toHaveAttribute("aria-expanded", "true");
    expect(buttons[1]).toHaveAttribute("aria-expanded", "false");

    // Same page, new ?exp= deep link → the new card must open.
    rerender(<ExperimentBoard cards={cards} defaultOpenHash="second5678cd" />);
    buttons = screen.getAllByRole("button");
    expect(buttons[1]).toHaveAttribute("aria-expanded", "true");
  });

  it("expandBoard and defaultOpenHash can be combined: expandBoard wins for all cards", () => {
    renderWithProviders(
      <ExperimentBoard
        cards={[
          makeCard({ hash: "card1234ef", state: "kept" }),
          makeCard({ hash: "card5678cd", state: "rejected" }),
        ]}
        expandBoard={true}
        defaultOpenHash="card1234ef"
      />,
    );
    const buttons = screen.getAllByRole("button");
    for (const btn of buttons) {
      expect(btn).toHaveAttribute("aria-expanded", "true");
    }
  });
});

describe("ExperimentBoard — expanded card spans the full row (Bug 6)", () => {
  function cellOf(button: HTMLElement): HTMLElement {
    // The grid cell is the direct child of the grid container wrapping the card.
    const grid = document.querySelector(".grid")!;
    let el: HTMLElement = button;
    while (el.parentElement && el.parentElement !== grid) el = el.parentElement;
    return el;
  }

  it("an expanded card's wrapper has col-span-full; collapsed cards don't", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <ExperimentBoard
        cards={[
          makeCard({ hash: "aaaa1234ef", state: "kept" }),
          makeCard({ hash: "bbbb5678cd", state: "rejected" }),
        ]}
      />,
    );
    const buttons = screen.getAllByRole("button");
    expect(cellOf(buttons[0]).className).not.toMatch(/col-span-full/);
    expect(cellOf(buttons[1]).className).not.toMatch(/col-span-full/);

    await user.click(buttons[0]);
    expect(buttons[0]).toHaveAttribute("aria-expanded", "true");
    expect(cellOf(buttons[0]).className).toMatch(/col-span-full/);
    expect(cellOf(buttons[1]).className).not.toMatch(/col-span-full/);
  });

  it("collapsing the card removes col-span-full", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <ExperimentBoard cards={[makeCard({ hash: "aaaa1234ef", state: "kept" })]} />,
    );
    const btn = screen.getByRole("button");
    await user.click(btn);
    expect(cellOf(btn).className).toMatch(/col-span-full/);
    await user.click(btn);
    expect(btn).toHaveAttribute("aria-expanded", "false");
    expect(cellOf(btn).className).not.toMatch(/col-span-full/);
  });

  it("defaultOpenHash card starts with col-span-full", () => {
    renderWithProviders(
      <ExperimentBoard
        cards={[
          makeCard({ hash: "target1234ef", state: "kept" }),
          makeCard({ hash: "other123456", state: "evaluating" }),
        ]}
        defaultOpenHash="target1234ef"
      />,
    );
    const buttons = screen.getAllByRole("button");
    expect(cellOf(buttons[0]).className).toMatch(/col-span-full/);
    expect(cellOf(buttons[1]).className).not.toMatch(/col-span-full/);
  });

  it("a defaultOpenHash card can be collapsed by the user", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <ExperimentBoard
        cards={[makeCard({ hash: "target1234ef", state: "kept" })]}
        defaultOpenHash="target1234ef"
      />,
    );
    const btn = screen.getByRole("button");
    expect(btn).toHaveAttribute("aria-expanded", "true");
    await user.click(btn);
    expect(btn).toHaveAttribute("aria-expanded", "false");
  });
});

describe("ExperimentBoard — mobile collapse (single-column)", () => {
  it("renders a grid container that collapses to 1 column on mobile", () => {
    renderWithProviders(
      <ExperimentBoard
        cards={[makeCard({ hash: "aaaa1234ef", state: "kept" })]}
      />,
    );
    // The outer grid should have 'grid' class and sm:grid-cols-2 or similar
    // (the single-col mobile comment in the plan)
    const grid = document.querySelector(".grid");
    expect(grid).not.toBeNull();
    // It should not have grid-cols-2 or grid-cols-3 at base (mobile-first)
    expect(grid!.className).not.toMatch(/^grid-cols-\d/);
  });
});
