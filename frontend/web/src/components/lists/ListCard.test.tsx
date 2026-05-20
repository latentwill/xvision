import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ListCard, type ListColumn } from "./ListCard";
import type { ListState } from "./useListState";

type Row = { id: string; name: string };

const ROWS: Row[] = [
  { id: "1", name: "ETH-MR" },
  { id: "2", name: "BTC-MOM" },
];

const COLUMNS: ListColumn[] = [
  { key: "name", label: "Name" },
  { key: "id", label: "ID", align: "right" },
];

function makeState(over: Partial<ListState<Row>> = {}): ListState<Row> {
  return {
    search: { value: "", setValue: vi.fn() },
    filters: [],
    sort: {
      value: "added",
      setValue: vi.fn(),
      options: [{ value: "added", label: "Recently added" }],
    },
    rows: ROWS,
    totalRows: ROWS.length,
    clearAll: vi.fn(),
    ...over,
  };
}

describe("ListCard", () => {
  it("renders header with title, count, and subtitle", () => {
    render(
      <ListCard<Row>
        title="Strategies"
        count={2}
        subtitle="2 strategies"
        columns={COLUMNS}
        rows={ROWS}
        renderRow={(r) => (
          <tr key={r.id}>
            <td>{r.name}</td>
            <td>{r.id}</td>
          </tr>
        )}
      />,
    );

    const heading = screen.getByRole("heading", { level: 2 });
    expect(heading).toHaveTextContent("Strategies");
    // Count pill sits next to the heading; assert by querying its parent block.
    const headerBlock = heading.parentElement!;
    expect(within(headerBlock).getByText("2")).toBeInTheDocument();
    expect(screen.getByText("2 strategies")).toBeInTheDocument();
  });

  it("renders populated rows when not loading / not empty", () => {
    render(
      <ListCard<Row>
        columns={COLUMNS}
        rows={ROWS}
        renderRow={(r) => (
          <tr key={r.id} data-testid={`row-${r.id}`}>
            <td>{r.name}</td>
            <td>{r.id}</td>
          </tr>
        )}
      />,
    );
    expect(screen.getByTestId("row-1")).toHaveTextContent("ETH-MR");
    expect(screen.getByTestId("row-2")).toHaveTextContent("BTC-MOM");
  });

  it("renders loading skeleton when loading=true", () => {
    const { container } = render(
      <ListCard<Row>
        columns={COLUMNS}
        rows={ROWS}
        renderRow={() => <tr />}
        loading
      />,
    );
    expect(container.querySelectorAll(".animate-pulse").length).toBeGreaterThan(
      0,
    );
    expect(screen.queryByText("ETH-MR")).not.toBeInTheDocument();
  });

  it("renders empty state with optional action when rows.length === 0", () => {
    render(
      <ListCard<Row>
        columns={COLUMNS}
        rows={[]}
        renderRow={() => <tr />}
        empty="No strategies yet."
        emptyAction={<button>New strategy</button>}
      />,
    );
    expect(screen.getByText("No strategies yet.")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "New strategy" }),
    ).toBeInTheDocument();
  });

  it("renders error state with retry callback wired", () => {
    const retry = vi.fn();
    render(
      <ListCard<Row>
        columns={COLUMNS}
        rows={ROWS}
        renderRow={() => <tr />}
        error={{ message: "Network error", retry }}
      />,
    );
    expect(screen.getByText("Network error")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it("toolbar remains rendered when body is in skeleton / empty / error state", () => {
    const state = makeState();
    const { rerender } = render(
      <ListCard<Row>
        columns={COLUMNS}
        rows={ROWS}
        renderRow={() => <tr />}
        toolbar={{ search: state.search, sort: state.sort }}
        loading
      />,
    );
    expect(screen.getByPlaceholderText(/search/i)).toBeInTheDocument();

    rerender(
      <ListCard<Row>
        columns={COLUMNS}
        rows={[]}
        renderRow={() => <tr />}
        toolbar={{ search: state.search, sort: state.sort }}
      />,
    );
    expect(screen.getByPlaceholderText(/search/i)).toBeInTheDocument();
  });

  it("active filter chip resets the filter when clicked", () => {
    const setStatus = vi.fn();
    const state = makeState({
      filters: [
        {
          def: {
            id: "status",
            label: "Status",
            options: [
              { value: "all", label: "All" },
              { value: "Validated", label: "Validated" },
            ],
          },
          value: "Validated",
          setValue: setStatus,
        },
      ],
    });

    render(
      <ListCard<Row>
        columns={COLUMNS}
        rows={ROWS}
        renderRow={() => <tr />}
        toolbar={{
          search: state.search,
          filters: state.filters,
          sort: state.sort,
          clearAll: state.clearAll,
        }}
      />,
    );

    const chips = screen.getByRole("region", { name: /active filters/i });
    fireEvent.click(within(chips).getByRole("button", { name: /Validated/ }));
    expect(setStatus).toHaveBeenCalledWith("all");
  });

  it("compact density hides the active-chips row and the / keyboard hint", () => {
    const state = makeState({
      search: { value: "eth", setValue: vi.fn() },
    });
    render(
      <ListCard<Row>
        density="compact"
        columns={COLUMNS}
        rows={ROWS}
        renderRow={() => <tr />}
        toolbar={{ search: state.search, sort: state.sort }}
      />,
    );
    expect(
      screen.queryByRole("region", { name: /active filters/i }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("/")).not.toBeInTheDocument();
  });
});
