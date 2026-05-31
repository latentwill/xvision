import { act, renderHook } from "@testing-library/react";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import {
  DEFAULT_PAGE_SIZE,
  PAGE_SIZE_OPTIONS,
  ServerPagerStrip,
  useServerPagination,
} from "./useServerPagination";

// ---------------------------------------------------------------------------
// useServerPagination - unit tests
// ---------------------------------------------------------------------------

describe("useServerPagination", () => {
  it("defaults to page 1 with limit=DEFAULT_PAGE_SIZE and offset=0", () => {
    const { result } = renderHook(() => useServerPagination(100));
    expect(result.current.page).toBe(1);
    expect(result.current.pageSize).toBe(DEFAULT_PAGE_SIZE);
    expect(result.current.limit).toBe(DEFAULT_PAGE_SIZE);
    expect(result.current.offset).toBe(0);
  });

  it("respects a custom initialPageSize", () => {
    const { result } = renderHook(() => useServerPagination(100, 25));
    expect(result.current.limit).toBe(25);
    expect(result.current.offset).toBe(0);
  });

  it("derives offset from (page - 1) * pageSize", () => {
    const { result } = renderHook(() => useServerPagination(200));
    act(() => result.current.setPage(3));
    expect(result.current.offset).toBe(100); // (3-1)*50
    expect(result.current.limit).toBe(50);
  });

  it("exposes the caller-supplied total unchanged", () => {
    const { result } = renderHook(() => useServerPagination(77));
    expect(result.current.total).toBe(77);
  });

  it("clamps page down when total shrinks below the current page", () => {
    const { result, rerender } = renderHook(
      ({ total }: { total: number }) => useServerPagination(total),
      { initialProps: { total: 200 } },
    );
    act(() => result.current.setPage(4)); // 4 pages exist at 50/page
    expect(result.current.page).toBe(4);
    expect(result.current.offset).toBe(150);

    rerender({ total: 60 }); // ceil(60/50)=2 pages; page 4 is now out of range
    expect(result.current.page).toBe(2);
    expect(result.current.offset).toBe(50);
  });

  it("keeps page 1 when total grows", () => {
    const { result, rerender } = renderHook(
      ({ total }: { total: number }) => useServerPagination(total),
      { initialProps: { total: 50 } },
    );
    expect(result.current.page).toBe(1);
    rerender({ total: 500 });
    expect(result.current.page).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// ServerPagerStrip - rendering and interaction tests
// ---------------------------------------------------------------------------

const makePagerProps = () => ({
  total: 200,
  page: 1,
  pageSize: 50,
  onPageChange: vi.fn(),
  onPageSizeChange: vi.fn(),
});

describe("ServerPagerStrip - hide threshold", () => {
  it("is hidden when total equals the smallest page-size option", () => {
    render(<ServerPagerStrip {...makePagerProps()} total={PAGE_SIZE_OPTIONS[0]} />);
    expect(screen.queryByTestId("list-pagination")).toBeNull();
  });

  it("is hidden for total=0", () => {
    render(<ServerPagerStrip {...makePagerProps()} total={0} />);
    expect(screen.queryByTestId("list-pagination")).toBeNull();
  });

  it("renders when total exceeds the smallest page-size option", () => {
    render(<ServerPagerStrip {...makePagerProps()} total={PAGE_SIZE_OPTIONS[0] + 1} />);
    expect(screen.getByTestId("list-pagination")).toBeInTheDocument();
  });
});

describe("ServerPagerStrip - next / previous interactions", () => {
  it("Previous button is disabled on page 1", () => {
    render(<ServerPagerStrip {...makePagerProps()} page={1} />);
    expect(screen.getByRole("button", { name: /previous page/i })).toBeDisabled();
  });

  it("Next button is disabled on the last page", () => {
    // total=100, pageSize=50 => 2 pages; page 2 is the last
    render(<ServerPagerStrip {...makePagerProps()} total={100} page={2} pageSize={50} />);
    expect(screen.getByRole("button", { name: /next page/i })).toBeDisabled();
  });

  it("Next button calls onPageChange with page+1", () => {
    const props = makePagerProps();
    render(<ServerPagerStrip {...props} page={1} total={200} />);
    fireEvent.click(screen.getByRole("button", { name: /next page/i }));
    expect(props.onPageChange).toHaveBeenCalledWith(2);
  });

  it("Previous button calls onPageChange with page-1", () => {
    const props = makePagerProps();
    render(<ServerPagerStrip {...props} page={3} total={200} />);
    fireEvent.click(screen.getByRole("button", { name: /previous page/i }));
    expect(props.onPageChange).toHaveBeenCalledWith(2);
  });
});

describe("ServerPagerStrip - page-size reset", () => {
  it("calls onPageSizeChange then resets to page 1 when page-size select changes", () => {
    const props = makePagerProps();
    render(
      <ServerPagerStrip
        {...props}
        page={3}
      />,
    );
    const select = screen.getByLabelText(/per page/i);
    fireEvent.change(select, { target: { value: "25" } });
    expect(props.onPageSizeChange).toHaveBeenCalledWith(25);
    expect(props.onPageChange).toHaveBeenCalledWith(1);
  });
});
