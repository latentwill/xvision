import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import { Routes, Route } from "react-router-dom";
import { renderWithProviders } from "../test-utils";
import { CycleDetail } from "./CycleDetail";
import * as client from "@/api/client";

// Mock ExpandableArtifact so board cards / feed lines don't pull the full
// experiment-detail network stack (same idiom as ConsoleModule.test.tsx).
// `aria-expanded` mirrors `defaultOpen` so deep-link tests can assert mount state.
vi.mock("../ui/ExpandableArtifact", () => ({
  ExpandableArtifact: ({
    hash,
    summary,
    defaultOpen,
  }: {
    hash: string;
    summary: React.ReactNode;
    defaultOpen?: boolean;
  }) => (
    <div
      data-testid={`artifact-${hash}`}
      role="button"
      aria-expanded={defaultOpen ? "true" : "false"}
    >
      {summary}
    </div>
  ),
}));

// No live SSE in jsdom: the console module must run in replay mode.
vi.mock("../hooks/useCycleEventStream", () => ({
  useCycleEventStream: () => ({
    events: [],
    connected: false,
    isRunning: false,
    activeCycleId: null,
  }),
}));

afterEach(() => vi.restoreAllMocks());

// ─── Fixtures ─────────────────────────────────────────────────────────────────

const node = (
  bundle_hash: string,
  status: string,
  delta_day?: number,
) => ({
  bundle_hash,
  parent_hash: "ffff0000aa",
  status,
  cycle_id: "cyc-1",
  created_at: "2026-06-01T00:30:00Z",
  regime_results: [],
  ...(delta_day !== undefined ? { delta_day } : {}),
});

const cycleDetail = {
  cycle_id: "cyc-1",
  node_count: 14,
  active_count: 2,
  suspect_count: 1,
  rejected_count: 11,
  first_created_at: "2026-06-01T00:00:00Z",
  last_created_at: "2026-06-01T01:00:00Z",
  cost_usd: 4.2,
  input_tokens: 1000,
  output_tokens: 500,
  unpriced_calls: 0,
  nodes: [
    node("abcd1234ef", "active", 0.21),
    node("aaaa1111bb", "active", 0.05),
    node("bbbb2222cc", "rejected", -0.4),
    node("cccc3333dd", "quarantined", 0.9),
  ],
};

const persistedRow = (seq: number, payload: Record<string, unknown>) => ({
  seq,
  session_id: "sess-1",
  cycle_id: "cyc-1",
  kind: String(payload.type),
  payload_json: JSON.stringify(payload),
  ts: "2026-06-01T00:10:00Z",
});

const persistedEvents = [
  persistedRow(1, { type: "cycle_started", cycle_id: "cyc-1", parent_count: 1 }),
  persistedRow(2, {
    type: "mutation_proposed",
    cycle_id: "cyc-1",
    parent_hash: "ffff0000aa",
    child_hash: "abcd1234ef",
    mutator_model: "gemini-2.5-pro",
  }),
  persistedRow(3, {
    type: "mutation_proposed",
    cycle_id: "cyc-1",
    parent_hash: "ffff0000aa",
    child_hash: "beef5678cd",
    mutator_model: "gpt-5.2",
  }),
  persistedRow(4, {
    type: "mutation_gated",
    child_hash: "abcd1234ef",
    passed: true,
    outcome: "kept",
    delta_day: 0.21,
  }),
  persistedRow(5, { type: "cycle_finished", active_count: 2, rejected_count: 11 }),
];

/** 120 persisted events — one cycle_started + 119 proposals. */
const manyEvents = [
  persistedRow(1, { type: "cycle_started", cycle_id: "cyc-1", parent_count: 1 }),
  ...Array.from({ length: 119 }, (_, i) =>
    persistedRow(i + 2, {
      type: "mutation_proposed",
      cycle_id: "cyc-1",
      parent_hash: "ffff0000aa",
      child_hash: `hash${String(i).padStart(6, "0")}`,
      mutator_model: "gemini-2.5-pro",
    }),
  ),
];

function mockApi(opts?: {
  detail?: Record<string, unknown>;
  events?: Record<string, unknown>[];
}) {
  const detail = opts?.detail ?? cycleDetail;
  const events = opts?.events ?? persistedEvents;
  return vi.spyOn(client, "apiFetch").mockImplementation(async (url: string) => {
    if (url.includes("/events")) return events;
    if (url.includes("/cycles/")) return detail;
    if (url.includes("/cycles")) return [detail];
    if (url.includes("/lineage")) return [];
    if (url.includes("/health")) return { status: "ok", probes: [] };
    return {};
  }) as unknown as ReturnType<typeof vi.spyOn>;
}

function renderCycleDetail(route = "/optimizer/cycle/cyc-1") {
  return renderWithProviders(
    <Routes>
      <Route path="/optimizer/cycle/:cycleId" element={<CycleDetail />} />
    </Routes>,
    { route },
  );
}

const h1Text = () =>
  screen
    .getAllByRole("heading", { level: 1 })
    .map((h) => h.textContent ?? "")
    .join(" ");

beforeEach(() => vi.clearAllMocks());

// ─── Tests ────────────────────────────────────────────────────────────────────

describe("CycleDetail", () => {
  it("renders the editorial headline with the best-find clause from the best kept node", async () => {
    mockApi();
    renderCycleDetail();

    await waitFor(() =>
      expect(h1Text()).toContain(
        "Cycle cyc-1 kept 2 of 14 experiments — best find abcd1234, ΔSharpe +0.21.",
      ),
    );
    // subtitle: $spend · n experiments (no digest line)
    expect(h1Text()).toContain("$4.20 · 14 experiments");
  });

  it("omits the best-find clause when no kept node carries a gate delta", async () => {
    mockApi({
      detail: {
        ...cycleDetail,
        active_count: 0,
        nodes: [node("bbbb2222cc", "rejected", -0.4), node("cccc3333dd", "quarantined", 0.9)],
      },
    });
    renderCycleDetail();

    await waitFor(() =>
      expect(h1Text()).toContain("Cycle cyc-1 kept 0 of 14 experiments."),
    );
    expect(h1Text()).not.toContain("best find");
  });

  it("renders a breadcrumb back to /optimizer", async () => {
    mockApi();
    renderCycleDetail();

    const nav = await screen.findByRole("navigation", { name: "Breadcrumb" });
    const link = within(nav).getByRole("link", { name: /optimizer/i });
    expect(link).toHaveAttribute("href", "/optimizer");
  });

  it("renders the console module in replay mode scoped to this cycle", async () => {
    const spy = mockApi();
    renderCycleDetail();

    // The persisted event log for THIS cycle is fetched…
    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith("/api/autooptimizer/cycles/cyc-1/events"),
    );
    // …and the feed narrates it.
    const feed = await screen.findByRole("list", { name: "Cycle events" });
    expect(within(feed).getAllByRole("listitem").length).toBe(persistedEvents.length);
  });

  it("opens the deep-linked board card via ?exp= on mount", async () => {
    mockApi();
    renderCycleDetail("/optimizer/cycle/cyc-1?exp=abcd1234ef");

    await waitFor(() => {
      const instances = screen.getAllByTestId("artifact-abcd1234ef");
      expect(instances.some((el) => el.getAttribute("aria-expanded") === "true")).toBe(true);
    });
  });

  it("mounts ALL board cards expanded when no ?exp= is present", async () => {
    mockApi();
    renderCycleDetail();

    await waitFor(() => {
      for (const hash of ["abcd1234ef", "beef5678cd"]) {
        const instances = screen.getAllByTestId(`artifact-${hash}`);
        expect(
          instances.some((el) => el.getAttribute("aria-expanded") === "true"),
        ).toBe(true);
      }
    });
  });

  it("renders the feed uncapped — all 120 events become rows", async () => {
    mockApi({ events: manyEvents });
    renderCycleDetail();

    const feed = await screen.findByRole("list", { name: "Cycle events" });
    await waitFor(() =>
      expect(within(feed).getAllByRole("listitem").length).toBe(120),
    );
  });

  it("retains the GateBuckets, EvalMatrix, experiments table and lineage tree panels", async () => {
    mockApi();
    renderCycleDetail();

    expect(await screen.findByText("Anti-overfit gate")).toBeInTheDocument();
    expect(screen.getByText("Eval matrix")).toBeInTheDocument();
    expect(screen.getByText("Experiments this cycle")).toBeInTheDocument();
    expect(screen.getByText("Lineage tree")).toBeInTheDocument();
  });
});
