import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { LIST_STD_DEFAULT_SORT, useListState } from "./useListState";

type Row = { id: string; name: string; status: string; added: number };

const ROWS: Row[] = [
  { id: "1", name: "ETH-MR", status: "Validated", added: 3 },
  { id: "2", name: "BTC-MOM", status: "Draft", added: 1 },
  { id: "3", name: "SOL-SCALP", status: "Validated", added: 2 },
];

describe("useListState", () => {
  it("defaults sort key to sortOptions[0] when initialSort is omitted", () => {
    const { result } = renderHook(() =>
      useListState<Row>({ rows: ROWS }),
    );
    expect(result.current.sort.value).toBe(LIST_STD_DEFAULT_SORT[0].value);
  });

  it("honors initialSort when provided", () => {
    const { result } = renderHook(() =>
      useListState<Row>({ rows: ROWS, initialSort: "name" }),
    );
    expect(result.current.sort.value).toBe("name");
  });

  it("returns rows unchanged when filterFn/sortFn are omitted", () => {
    const { result } = renderHook(() =>
      useListState<Row>({ rows: ROWS }),
    );
    expect(result.current.rows).toEqual(ROWS);
    expect(result.current.totalRows).toBe(3);
  });

  it("applies filterFn against search + filter values", () => {
    const { result } = renderHook(() =>
      useListState<Row>({
        rows: ROWS,
        filters: [
          {
            id: "status",
            label: "Status",
            options: [
              { value: "all", label: "All" },
              { value: "Validated", label: "Validated" },
              { value: "Draft", label: "Draft" },
            ],
          },
        ],
        filterFn: (row, query, values) => {
          const statusOk = values.status === "all" || row.status === values.status;
          const searchOk = !query || row.name.toLowerCase().includes(query.toLowerCase());
          return statusOk && searchOk;
        },
      }),
    );

    expect(result.current.rows).toHaveLength(3);

    act(() => result.current.filters[0].setValue("Validated"));
    expect(result.current.rows.map((r) => r.id)).toEqual(["1", "3"]);

    act(() => result.current.search.setValue("eth"));
    expect(result.current.rows.map((r) => r.id)).toEqual(["1"]);
  });

  it("re-derives rows when sortKey changes", () => {
    const { result } = renderHook(() =>
      useListState<Row>({
        rows: ROWS,
        sortFn: (rows, key) =>
          key === "name"
            ? [...rows].sort((a, b) => a.name.localeCompare(b.name))
            : [...rows].sort((a, b) => b.added - a.added),
      }),
    );

    expect(result.current.rows.map((r) => r.id)).toEqual(["1", "3", "2"]);

    act(() => result.current.sort.setValue("name"));
    expect(result.current.rows.map((r) => r.name)).toEqual([
      "BTC-MOM",
      "ETH-MR",
      "SOL-SCALP",
    ]);
  });

  it("clearAll resets search and filters to defaults", () => {
    const { result } = renderHook(() =>
      useListState<Row>({
        rows: ROWS,
        filters: [
          {
            id: "status",
            label: "Status",
            options: [
              { value: "all", label: "All" },
              { value: "Validated", label: "Validated" },
            ],
          },
        ],
        filterFn: () => true,
      }),
    );

    act(() => {
      result.current.search.setValue("eth");
      result.current.filters[0].setValue("Validated");
    });
    expect(result.current.search.value).toBe("eth");
    expect(result.current.filters[0].value).toBe("Validated");

    act(() => result.current.clearAll());
    expect(result.current.search.value).toBe("");
    expect(result.current.filters[0].value).toBe("all");
  });

  it("respects filter defaultValue when supplied", () => {
    const { result } = renderHook(() =>
      useListState<Row>({
        rows: ROWS,
        filters: [
          {
            id: "status",
            label: "Status",
            defaultValue: "Validated",
            options: [
              { value: "all", label: "All" },
              { value: "Validated", label: "Validated" },
              { value: "Draft", label: "Draft" },
            ],
          },
        ],
      }),
    );
    expect(result.current.filters[0].value).toBe("Validated");
  });
});
