// Safety route and SafetyPauseBadge tests.
//
// Asserts:
// 1. Pause indicator renders inline in the Topbar (no popup) when paused.
// 2. Running state hides the badge.
// 3. VenueBadge renders inline (no popup) with correct tone per label.
// 4. SafetyRoute shows state card + audit rows inline (no popups).

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, act } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";

import { getSafetyState, getSafetyAudit } from "@/api/safety";
import { SafetyPauseBadge } from "@/components/shell/SafetyPauseBadge";
import { VenueBadge } from "@/components/primitives/VenueBadge";
import { SafetyRoute } from "./safety";

// ── Module mocks ──────────────────────────────────────────────────────────────

vi.mock("@/api/safety", async () => {
  const actual = await vi.importActual<typeof import("@/api/safety")>(
    "@/api/safety",
  );
  return {
    ...actual,
    getSafetyState: vi.fn(),
    getSafetyAudit: vi.fn(),
    pauseSafety: vi.fn(),
    resumeSafety: vi.fn(),
  };
});

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeQC() {
  return new QueryClient({ defaultOptions: { queries: { retry: false } } });
}

function wrapper(ui: React.ReactElement, qc = makeQC()) {
  return render(
    <MemoryRouter>
      <QueryClientProvider client={qc}>{ui}</QueryClientProvider>
    </MemoryRouter>,
  );
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

// ── SafetyPauseBadge ─────────────────────────────────────────────────────────

describe("SafetyPauseBadge", () => {
  it("renders no badge when safety is running", async () => {
    vi.mocked(getSafetyState).mockResolvedValue({ paused: false });
    wrapper(<SafetyPauseBadge />);
    // Wait for query to settle — nothing should appear.
    await act(async () => {});
    expect(screen.queryByTestId("safety-pause-badge")).toBeNull();
  });

  it("renders pause badge inline (not in a dialog/modal) when paused", async () => {
    vi.mocked(getSafetyState).mockResolvedValue({
      paused: true,
      reason: "test pause",
    });
    wrapper(<SafetyPauseBadge />);
    const badge = await screen.findByTestId("safety-pause-badge");
    // Inline: must NOT be inside a dialog, alertdialog, or modal element.
    const parent = badge.closest("[role='dialog'], [role='alertdialog']");
    expect(parent).toBeNull();
    // Content check.
    expect(badge).toHaveTextContent("paused");
  });
});

// ── VenueBadge ────────────────────────────────────────────────────────────────

describe("VenueBadge", () => {
  it("renders paper badge inline", () => {
    wrapper(<VenueBadge label="paper" />);
    const badge = screen.getByTestId("venue-badge-paper");
    expect(badge).toHaveTextContent("paper");
    // Inline — no dialog ancestor.
    expect(badge.closest("[role='dialog'], [role='alertdialog']")).toBeNull();
  });

  it("renders testnet badge inline", () => {
    wrapper(<VenueBadge label="testnet" />);
    expect(screen.getByTestId("venue-badge-testnet")).toHaveTextContent("testnet");
  });

  it("renders live badge inline", () => {
    wrapper(<VenueBadge label="live" />);
    expect(screen.getByTestId("venue-badge-live")).toHaveTextContent("live");
  });
});

// ── SafetyRoute ───────────────────────────────────────────────────────────────

describe("SafetyRoute", () => {
  const mockAudit = [
    {
      id: 1,
      timestamp: "2026-05-21T10:00:00Z",
      user: "test",
      source: "api",
      action_kind: "broker_submit",
      params_json: "{}",
      result: "allowed",
      pause_state_at_time: false,
    },
    {
      id: 2,
      timestamp: "2026-05-21T10:01:00Z",
      user: "test",
      source: "api",
      action_kind: "pause_toggle",
      params_json: "{}",
      result: "denied_safety_paused",
      pause_state_at_time: true,
    },
  ];

  it("shows running state inline (no popups)", async () => {
    vi.mocked(getSafetyState).mockResolvedValue({ paused: false });
    vi.mocked(getSafetyAudit).mockResolvedValue(mockAudit);
    wrapper(<SafetyRoute />);
    const pill = await screen.findByTestId("safety-state-running");
    // Must be inline — no dialog ancestor.
    expect(pill.closest("[role='dialog'], [role='alertdialog']")).toBeNull();
  });

  it("shows paused state inline (no popups)", async () => {
    vi.mocked(getSafetyState).mockResolvedValue({
      paused: true,
      reason: "deliberate stop",
    });
    vi.mocked(getSafetyAudit).mockResolvedValue([]);
    wrapper(<SafetyRoute />);
    const pill = await screen.findByTestId("safety-state-paused");
    expect(pill.closest("[role='dialog'], [role='alertdialog']")).toBeNull();
    expect(pill).toHaveTextContent("paused");
  });

  it("renders audit rows in a table (inline, no popups)", async () => {
    vi.mocked(getSafetyState).mockResolvedValue({ paused: false });
    vi.mocked(getSafetyAudit).mockResolvedValue(mockAudit);
    wrapper(<SafetyRoute />);
    const rows = await screen.findAllByTestId("audit-row");
    expect(rows).toHaveLength(2);
    // None are inside a dialog.
    for (const row of rows) {
      expect(row.closest("[role='dialog'], [role='alertdialog']")).toBeNull();
    }
  });
});
