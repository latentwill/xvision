import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { OriginDiffPanel } from "./OriginDiffPanel";
import * as api from "../api";

vi.mock("../api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api")>();
  return { ...actual, useOriginDiff: vi.fn() };
});

const wrap = ({ children }: { children: React.ReactNode }) => (
  <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
    {children}
  </QueryClientProvider>
);

describe("OriginDiffPanel", () => {
  beforeEach(() => vi.clearAllMocks());

  it("shows loading state", () => {
    vi.mocked(api.useOriginDiff).mockReturnValue({
      data: undefined, isLoading: true, isError: false,
    } as unknown as ReturnType<typeof api.useOriginDiff>);
    render(<OriginDiffPanel hash="abc123" />, { wrapper: wrap });
    expect(screen.getByText(/loading/i)).toBeTruthy();
  });

  it("renders prose changes", () => {
    vi.mocked(api.useOriginDiff).mockReturnValue({
      data: {
        origin_hash: "deadbeef",
        diff: {
          prose: [{ agent_role: "trader", before: "buy low", after: "sell high" }],
          params: [], tools: { added: [], removed: [] }, filter: [],
        },
      },
      isLoading: false, isError: false,
    } as unknown as ReturnType<typeof api.useOriginDiff>);
    render(<OriginDiffPanel hash="abc123" />, { wrapper: wrap });
    expect(screen.getByText("sell high")).toBeTruthy();
  });

  it("shows empty state when no changes", () => {
    vi.mocked(api.useOriginDiff).mockReturnValue({
      data: {
        origin_hash: "deadbeef",
        diff: { prose: [], params: [], tools: { added: [], removed: [] }, filter: [] },
      },
      isLoading: false, isError: false,
    } as unknown as ReturnType<typeof api.useOriginDiff>);
    render(<OriginDiffPanel hash="abc123" />, { wrapper: wrap });
    expect(screen.getByText(/no changes/i)).toBeTruthy();
  });
});
