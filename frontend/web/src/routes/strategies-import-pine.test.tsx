import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { StrategiesImportPineRoute } from "./strategies-import-pine";
import * as pineApi from "@/api/pine-import";

// --- minimal mock of navigate so we can assert on it ---
const navigate = vi.fn();

vi.mock("react-router-dom", async () => {
  const actual = await vi.importActual<typeof import("react-router-dom")>(
    "react-router-dom",
  );
  return {
    ...actual,
    useNavigate: () => navigate,
  };
});

vi.mock("@/api/pine-import", async () => {
  const actual = await vi.importActual<typeof import("@/api/pine-import")>(
    "@/api/pine-import",
  );
  return {
    ...actual,
    importPineScript: vi.fn(),
  };
});

// --- fixtures ---

const SUCCESS_RESPONSE: pineApi.PineImportResult = {
  strategy: {
    manifest: {
      id: "01JTEST0000000000000000001",
      display_name: "EMA Cross",
      plain_summary: "Imported from Pine",
      creator: "",
      template: "mechanistic",
      regime_fit: [],
      asset_universe: ["BTCUSDT"],
      decision_cadence_minutes: 60,
      attested_with: [],
      required_tools: [],
      risk_preset_or_config: "default",
      published_at: null,
    },
    regime_slot: null,
    trader_slot: null,
    risk: {
      risk_pct_per_trade: 1,
      max_concurrent_positions: 1,
      max_leverage: 1,
      stop_loss_atr_multiple: 2,
      daily_loss_kill_pct: 5,
    },
    decision_mode: "mechanistic",
  },
  fidelity_report: {
    captured: [
      { item: "ema_cross_entry", reason: "Mapped to EMA crossover condition" },
    ],
    approximated: [
      {
        item: "close*1.02",
        reason: "Approximated as within_pct_2",
      },
    ],
    dropped: [
      { item: "pyramiding", reason: "Not supported: multiple entries per direction" },
    ],
    cost_model: {
      commission_type: "percentage",
      commission_value_bps: 10,
      slippage_model: "fixed",
      slippage_value_bps: 5,
      fill_timing: "next_bar_open",
      note: "xvision uses T+1 bar-open fills; TradingView defaults to bar-close. P&L will differ.",
    },
  },
};

function renderRoute() {
  return render(
    <MemoryRouter initialEntries={["/strategies/import-pine"]}>
      <QueryClientProvider
        client={
          new QueryClient({
            defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
          })
        }
      >
        <StrategiesImportPineRoute />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  navigate.mockReset();
  vi.mocked(pineApi.importPineScript).mockResolvedValue(SUCCESS_RESPONSE);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

// ───────────────────────────────────────────────────────────────────────────
// Happy path — successful import renders all three fidelity sections
// ───────────────────────────────────────────────────────────────────────────
describe("StrategiesImportPineRoute — happy path", () => {
  it("renders the textarea and submit button on mount", () => {
    renderRoute();
    expect(screen.getByRole("textbox", { name: /pine script source/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /import/i })).toBeInTheDocument();
  });

  it("calls importPineScript with the pasted source on submit", async () => {
    const user = userEvent.setup();
    renderRoute();

    await user.type(
      screen.getByRole("textbox", { name: /pine script source/i }),
      "//@version=5",
    );
    await user.click(screen.getByRole("button", { name: /import/i }));

    await waitFor(() => {
      expect(pineApi.importPineScript).toHaveBeenCalledWith({
        source: "//@version=5",
        name: undefined,
      });
    });
  });

  it("forwards an optional strategy name when provided", async () => {
    const user = userEvent.setup();
    renderRoute();

    await user.type(screen.getByRole("textbox", { name: /pine script source/i }), "//@version=5");
    await user.type(screen.getByRole("textbox", { name: /strategy name/i }), "My Pine Strategy");
    await user.click(screen.getByRole("button", { name: /import/i }));

    await waitFor(() => {
      expect(pineApi.importPineScript).toHaveBeenCalledWith({
        source: "//@version=5",
        name: "My Pine Strategy",
      });
    });
  });

  it("renders the Captured fidelity section with items after success", async () => {
    const user = userEvent.setup();
    renderRoute();

    await user.type(screen.getByRole("textbox", { name: /pine script source/i }), "//@version=5");
    await user.click(screen.getByRole("button", { name: /import/i }));

    expect(await screen.findByText(/captured/i)).toBeInTheDocument();
    expect(await screen.findByText("ema_cross_entry")).toBeInTheDocument();
    expect(await screen.findByText("Mapped to EMA crossover condition")).toBeInTheDocument();
  });

  it("renders the Approximated fidelity section with items after success", async () => {
    const user = userEvent.setup();
    renderRoute();

    await user.type(screen.getByRole("textbox", { name: /pine script source/i }), "//@version=5");
    await user.click(screen.getByRole("button", { name: /import/i }));

    // Find the section heading specifically (role=heading)
    const headings = await screen.findAllByRole("heading", { name: /approximated/i });
    expect(headings.length).toBeGreaterThan(0);
    expect(await screen.findByText("close*1.02")).toBeInTheDocument();
    expect(await screen.findByText("Approximated as within_pct_2")).toBeInTheDocument();
  });

  it("renders the Dropped fidelity section with items after success", async () => {
    const user = userEvent.setup();
    renderRoute();

    await user.type(screen.getByRole("textbox", { name: /pine script source/i }), "//@version=5");
    await user.click(screen.getByRole("button", { name: /import/i }));

    expect(await screen.findByText(/dropped/i)).toBeInTheDocument();
    expect(await screen.findByText("pyramiding")).toBeInTheDocument();
    expect(await screen.findByText("Not supported: multiple entries per direction")).toBeInTheDocument();
  });

  it("renders the cost model strip with commission / slippage / fill-timing details", async () => {
    const user = userEvent.setup();
    renderRoute();

    await user.type(screen.getByRole("textbox", { name: /pine script source/i }), "//@version=5");
    await user.click(screen.getByRole("button", { name: /import/i }));

    // commission label (use exact label text)
    const commissionLabels = await screen.findAllByText(/commission:/i);
    expect(commissionLabels.length).toBeGreaterThan(0);
    // slippage label
    const slippageLabels = await screen.findAllByText(/slippage:/i);
    expect(slippageLabels.length).toBeGreaterThan(0);
    // fill timing value (the actual value rendered in the component)
    expect(await screen.findByText("next_bar_open")).toBeInTheDocument();
    // cost model note
    expect(
      await screen.findByText(/T\+1 bar-open fills/i),
    ).toBeInTheDocument();
  });

  it("renders a link to the created strategy", async () => {
    const user = userEvent.setup();
    renderRoute();

    await user.type(screen.getByRole("textbox", { name: /pine script source/i }), "//@version=5");
    await user.click(screen.getByRole("button", { name: /import/i }));

    const link = await screen.findByRole("link", { name: /view strategy/i });
    expect(link).toHaveAttribute(
      "href",
      `/strategies/${encodeURIComponent(SUCCESS_RESPONSE.strategy.manifest.id)}`,
    );
  });

  it("renders the 'Optimize this' CTA linking to the optimizer for the strategy", async () => {
    const user = userEvent.setup();
    renderRoute();

    await user.type(screen.getByRole("textbox", { name: /pine script source/i }), "//@version=5");
    await user.click(screen.getByRole("button", { name: /import/i }));

    const cta = await screen.findByRole("link", { name: /optimize this/i });
    expect(cta).toHaveAttribute("href", expect.stringContaining("/optimizer"));
  });

  it("does NOT render any dialog / modal / overlay element", async () => {
    const user = userEvent.setup();
    renderRoute();

    await user.type(screen.getByRole("textbox", { name: /pine script source/i }), "//@version=5");
    await user.click(screen.getByRole("button", { name: /import/i }));

    // Wait for success state to render
    await screen.findByText(/captured/i);

    // Overlay / modal / dialog roles must not be in the tree
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });
});

// ───────────────────────────────────────────────────────────────────────────
// Error path — structured 400 error renders inline, no overlay
// ───────────────────────────────────────────────────────────────────────────
describe("StrategiesImportPineRoute — error path", () => {
  it("renders the inline error when the API returns a 400", async () => {
    vi.mocked(pineApi.importPineScript).mockRejectedValue(
      new Error("parse error at line 3: unexpected token '+'"),
    );
    const user = userEvent.setup();
    renderRoute();

    await user.type(screen.getByRole("textbox", { name: /pine script source/i }), "bad script");
    await user.click(screen.getByRole("button", { name: /import/i }));

    expect(
      await screen.findByText(/parse error at line 3/i),
    ).toBeInTheDocument();
  });

  it("does NOT render any dialog / modal / overlay element on error", async () => {
    vi.mocked(pineApi.importPineScript).mockRejectedValue(
      new Error("parse error at line 3: unexpected token"),
    );
    const user = userEvent.setup();
    renderRoute();

    await user.type(screen.getByRole("textbox", { name: /pine script source/i }), "bad script");
    await user.click(screen.getByRole("button", { name: /import/i }));

    await screen.findByRole("alert");

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it("does NOT render fidelity sections on error", async () => {
    vi.mocked(pineApi.importPineScript).mockRejectedValue(
      new Error("parse failure - unexpected syntax"),
    );
    const user = userEvent.setup();
    renderRoute();

    await user.type(screen.getByRole("textbox", { name: /pine script source/i }), "bad script");
    await user.click(screen.getByRole("button", { name: /import/i }));

    // Wait for the error alert to appear
    await screen.findByRole("alert");

    // Fidelity section headings (rendered as h3) should not appear
    expect(screen.queryByRole("heading", { name: /captured/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: /approximated/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: /dropped/i })).not.toBeInTheDocument();
  });
});
