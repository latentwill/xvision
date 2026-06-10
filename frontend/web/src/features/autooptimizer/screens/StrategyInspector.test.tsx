import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { StrategyInspector } from "./StrategyInspector";
import * as api from "../api";

vi.mock("../api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api")>();
  return { ...actual, useBlob: vi.fn(), useLineageNode: vi.fn(), promoteStrategy: vi.fn() };
});

// Mock child panels to keep tests focused
vi.mock("../panels/OriginDiffPanel", () => ({ OriginDiffPanel: () => <div>origin-diff</div> }));
vi.mock("../panels/ParentDiffPanel", () => ({ ParentDiffPanel: () => <div>parent-diff</div> }));

const wrap = ({ children }: { children: React.ReactNode }) => (
  <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
    <MemoryRouter initialEntries={["/optimizer/strategy/abc123"]}>
      <Routes>
        <Route path="/optimizer/strategy/:hash" element={<>{children}</>} />
        <Route path="/strategies" element={<div>strategies-page</div>} />
      </Routes>
    </MemoryRouter>
  </QueryClientProvider>
);

const mockLoaded = () => {
  vi.mocked(api.useBlob).mockReturnValue({
    data: { manifest: { display_name: "Test Strategy", id: "s1" } },
    isLoading: false, isError: false,
  } as ReturnType<typeof api.useBlob>);
  vi.mocked(api.useLineageNode).mockReturnValue({
    data: { bundle_hash: "abc123", parent_hash: null, gate_verdict: "Pass", status: "active", cycle_id: null, created_at: "2026-01-01" },
    isLoading: false, isError: false,
  } as ReturnType<typeof api.useLineageNode>);
};

describe("StrategyInspector", () => {
  beforeEach(() => vi.clearAllMocks());

  it("renders strategy display name from blob", () => {
    mockLoaded();
    render(<StrategyInspector />, { wrapper: wrap });
    expect(screen.getByText("Test Strategy")).toBeTruthy();
  });

  it("shows Promote to Eval button", () => {
    mockLoaded();
    render(<StrategyInspector />, { wrapper: wrap });
    expect(screen.getByRole("button", { name: /promote to eval/i })).toBeTruthy();
  });

  it("navigates to /strategies after successful promote", async () => {
    mockLoaded();
    vi.mocked(api.promoteStrategy).mockResolvedValue({ strategy_id: "opt-abc12300" });
    render(<StrategyInspector />, { wrapper: wrap });
    fireEvent.click(screen.getByRole("button", { name: /promote to eval/i }));
    await waitFor(() => screen.getByText("strategies-page"));
  });
});
