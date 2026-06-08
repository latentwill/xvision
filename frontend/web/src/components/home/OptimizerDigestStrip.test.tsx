import { describe, expect, it, vi, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { OptimizerDigestStrip } from "./OptimizerDigestStrip";
import * as apiModule from "@/features/autooptimizer/api";
import type { SessionListItem } from "@/features/autooptimizer/api";

afterEach(() => vi.restoreAllMocks());

function renderStrip() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <OptimizerDigestStrip />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const baseSession: SessionListItem & { suspect_count?: number } = {
  session_id: "sess_01TESTABCDEF",
  strategy_id: "strat-abc",
  state: "finished",
  mode: "explore",
  cycles_completed: 12,
  kept_count: 5,
  suspect_count: 2,
  cost_usd: 0.47,
  finished_at: "2026-06-07T10:00:00Z",
};

describe("OptimizerDigestStrip", () => {
  // Test 1: Returns null when sessions list is empty
  it("returns null when sessions list is empty", () => {
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
      isPending: false,
      isSuccess: true,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);

    const { container } = renderStrip();
    expect(container.firstChild).toBeNull();
  });

  // Test 2: Returns null while sessions are loading (undefined data)
  it("returns null while sessions are loading", () => {
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: undefined,
      isLoading: true,
      isError: false,
      isPending: true,
      isSuccess: false,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);

    const { container } = renderStrip();
    expect(container.firstChild).toBeNull();
  });

  // Test 3: Renders "X experiments · Y kept · Z suspect" from fixture SessionListItem
  it("renders experiments, kept, and suspect counts from the most recent session", () => {
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [baseSession] as SessionListItem[],
      isLoading: false,
      isError: false,
      isPending: false,
      isSuccess: true,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);

    renderStrip();
    const strip = screen.getByTestId("optimizer-digest-strip");
    expect(strip.textContent).toContain("12 experiments");
    expect(strip.textContent).toContain("5 kept");
    expect(strip.textContent).toContain("2 suspect");
  });

  // Test 4: Shows cost formatted to 2 decimal places
  it("shows cost formatted to 2 decimal places", () => {
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [baseSession] as SessionListItem[],
      isLoading: false,
      isError: false,
      isPending: false,
      isSuccess: true,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);

    renderStrip();
    const strip = screen.getByTestId("optimizer-digest-strip");
    expect(strip.textContent).toContain("$0.47");
  });

  // Test 4b: Shows "?" when cost_usd is undefined
  it("shows '?' when cost_usd is undefined", () => {
    const sessionNoCost: SessionListItem = { ...baseSession, cost_usd: undefined };
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [sessionNoCost] as SessionListItem[],
      isLoading: false,
      isError: false,
      isPending: false,
      isSuccess: true,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);

    renderStrip();
    const strip = screen.getByTestId("optimizer-digest-strip");
    expect(strip.textContent).toContain("$?");
  });

  // Test 5: "Honesty check" text present (exact string, not "canary")
  it("shows 'Honesty check' text and never uses 'canary'", () => {
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [baseSession] as SessionListItem[],
      isLoading: false,
      isError: false,
      isPending: false,
      isSuccess: true,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);

    renderStrip();
    const strip = screen.getByTestId("optimizer-digest-strip");
    expect(strip.textContent).toContain("Honesty check");
    expect(strip.textContent).not.toContain("canary");
  });

  // Test 6: Link to /optimizer/run/:session_id present
  it("renders a link to /optimizer/run/:session_id", () => {
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [baseSession] as SessionListItem[],
      isLoading: false,
      isError: false,
      isPending: false,
      isSuccess: true,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);

    renderStrip();
    const link = screen.getByRole("link", { name: /view run/i });
    expect(link).toHaveAttribute("href", "/optimizer/run/sess_01TESTABCDEF");
  });

  // ─── S0 / O1a + O1b: real suspect + honesty rendering ─────────────────────

  function mockSession(session: SessionListItem) {
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [session] as SessionListItem[],
      isLoading: false,
      isError: false,
      isPending: false,
      isSuccess: true,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);
  }

  it("renders the real suspect_count from the typed field (O1a)", () => {
    mockSession({ ...baseSession, suspect_count: 4 });
    renderStrip();
    expect(screen.getByTestId("optimizer-digest-strip").textContent).toContain("4 suspect");
  });

  it("shows 'Honesty check ✓' when the latest cycle passed (O1b)", () => {
    mockSession({ ...baseSession, honesty_passed: true });
    renderStrip();
    expect(screen.getByTestId("optimizer-digest-strip").textContent).toContain("Honesty check ✓");
  });

  it("shows 'Honesty check ✗ failed' when the latest cycle failed (O1b)", () => {
    mockSession({ ...baseSession, honesty_passed: false });
    renderStrip();
    expect(screen.getByTestId("optimizer-digest-strip").textContent).toContain(
      "Honesty check ✗ failed",
    );
  });

  it("shows 'Honesty check —' when no honesty signal is present (O1b)", () => {
    mockSession({ ...baseSession, honesty_passed: undefined });
    renderStrip();
    expect(screen.getByTestId("optimizer-digest-strip").textContent).toContain("Honesty check —");
  });
});
