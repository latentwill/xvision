import { fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { MListCard } from "./MListCard";
import { MListRow } from "./MListRow";
import type {
  ActiveFilter,
  ListSearchState,
  ListSortState,
} from "./useListState";

type Row = { id: string; name: string };

const ROWS: Row[] = [
  { id: "1", name: "ETH-MR" },
  { id: "2", name: "BTC-MOM" },
];

function makeSearch(value = ""): ListSearchState {
  return { value, setValue: vi.fn() };
}

function makeSort(value = "added"): ListSortState {
  return {
    value,
    setValue: vi.fn(),
    options: [
      { value: "added", label: "Recently added" },
      { value: "name", label: "Name A → Z" },
    ],
  };
}

function makeFilter(value = "all", setValue = vi.fn()): ActiveFilter {
  return {
    def: {
      id: "status",
      label: "Status",
      options: [
        { value: "all", label: "All" },
        { value: "Validated", label: "Validated" },
      ],
    },
    value,
    setValue,
  };
}

afterEach(() => {
  document.body.style.overflow = "";
  document.body.style.paddingRight = "";
});

describe("MListCard", () => {
  it("renders header, count, populated rows", () => {
    render(
      <MListCard<Row>
        title="Strategies"
        count={2}
        toolbar={{ search: makeSearch(), sort: makeSort() }}
        rows={ROWS}
        renderRow={(r) => (
          <div key={r.id} data-testid={`row-${r.id}`}>
            {r.name}
          </div>
        )}
      />,
    );
    expect(screen.getByRole("heading", { level: 2 })).toHaveTextContent(
      "Strategies",
    );
    expect(screen.getByText("2")).toBeInTheDocument();
    expect(screen.getByTestId("row-1")).toHaveTextContent("ETH-MR");
  });

  it("renders loading skeletons when loading=true", () => {
    const { container } = render(
      <MListCard<Row>
        title="Strategies"
        toolbar={{ search: makeSearch(), sort: makeSort() }}
        rows={ROWS}
        renderRow={() => null}
        loading
      />,
    );
    expect(container.querySelectorAll(".animate-pulse").length).toBeGreaterThan(
      0,
    );
  });

  it("renders empty state with optional action", () => {
    render(
      <MListCard<Row>
        title="Strategies"
        toolbar={{ search: makeSearch(), sort: makeSort() }}
        rows={[]}
        renderRow={() => null}
        empty="No strategies yet."
        emptyAction={<button>New strategy</button>}
      />,
    );
    expect(screen.getByText("No strategies yet.")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "New strategy" }),
    ).toBeInTheDocument();
  });

  it("renders error state with retry", () => {
    const retry = vi.fn();
    render(
      <MListCard<Row>
        title="Strategies"
        toolbar={{ search: makeSearch(), sort: makeSort() }}
        rows={ROWS}
        renderRow={() => null}
        error={{ message: "Network error", retry }}
      />,
    );
    expect(screen.getByText("Network error")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it("Filter pill opens sheet in filters mode", () => {
    render(
      <MListCard<Row>
        title="Strategies"
        toolbar={{
          search: makeSearch(),
          filters: [makeFilter()],
          sort: makeSort(),
        }}
        rows={ROWS}
        renderRow={() => null}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /^Filter$/ }));
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveAttribute("aria-label", "Filter and sort");
    expect(within(dialog).getByText("Status")).toBeInTheDocument();
  });

  it("Sort pill opens sheet in sort-focus mode", () => {
    render(
      <MListCard<Row>
        title="Strategies"
        toolbar={{
          search: makeSearch(),
          filters: [makeFilter()],
          sort: makeSort(),
        }}
        rows={ROWS}
        renderRow={() => null}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /Recently added/ }));
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveAttribute("aria-label", "Sort by");
    expect(within(dialog).queryByText("Status")).not.toBeInTheDocument();
  });

  it("filter pill shows badge with active count and tints gold", () => {
    render(
      <MListCard<Row>
        title="Strategies"
        toolbar={{
          search: makeSearch(),
          filters: [makeFilter("Validated")],
          sort: makeSort(),
        }}
        rows={ROWS}
        renderRow={() => null}
      />,
    );
    const filterBtn = screen.getByRole("button", { name: /^Filter/i });
    expect(filterBtn).toHaveAttribute("data-active");
    // Badge "1" lives inside the filter pill as the count.
    expect(within(filterBtn).getByText(/^1$/)).toBeInTheDocument();
  });

  it("active chip click resets that filter", () => {
    const setStatus = vi.fn();
    render(
      <MListCard<Row>
        title="Strategies"
        toolbar={{
          search: makeSearch(),
          filters: [makeFilter("Validated", setStatus)],
          sort: makeSort(),
        }}
        rows={ROWS}
        renderRow={() => null}
      />,
    );
    const region = screen.getByRole("region", { name: /active filters/i });
    fireEvent.click(within(region).getByRole("button", { name: /Validated/ }));
    expect(setStatus).toHaveBeenCalledWith("all");
  });

  it("renders numeric zero for title and subtitle (nullish guard)", () => {
    render(
      <MListCard<Row>
        title={0}
        subtitle={0}
        toolbar={{ search: makeSearch(), sort: makeSort() }}
        rows={[]}
        renderRow={() => null}
      />,
    );
    expect(screen.getByRole("heading", { level: 2 })).toHaveTextContent("0");
    expect(screen.getAllByText("0")).toHaveLength(2);
  });

  it("MListRow renders zero-valued optional slots", () => {
    render(
      <MListRow
        title="Cash"
        badge={0}
        subtitle={0}
        meta={0}
        rightTop={0}
        rightSub={0}
      />,
    );

    expect(screen.getAllByText("0")).toHaveLength(5);
  });
});
