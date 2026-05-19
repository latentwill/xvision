import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";

import { InlineEditField } from "./InlineEditField";
import { StrategyDetailRoute } from "@/routes/strategies-detail";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

// ─────────────────────────────────────────────────────────────────────────
// InlineEditField — component-level contract.

describe("InlineEditField", () => {
  it("renders the display value in a button until clicked", () => {
    render(
      <InlineEditField
        id="t"
        label="Title"
        value="Original Title"
        onSave={vi.fn()}
      />,
    );
    const display = screen.getByTestId("inline-edit-display-t");
    expect(display).toHaveTextContent("Original Title");
    expect(display.tagName).toBe("BUTTON");
    // No input mounted in display mode.
    expect(screen.queryByTestId("inline-edit-input-t")).toBeNull();
  });

  it("enters edit mode on click and pre-fills the input with the current value", async () => {
    const user = userEvent.setup();
    render(
      <InlineEditField
        id="t"
        label="Title"
        value="Original Title"
        onSave={vi.fn()}
      />,
    );
    await user.click(screen.getByTestId("inline-edit-display-t"));
    const input = screen.getByTestId("inline-edit-input-t") as HTMLInputElement;
    expect(input).toBeInTheDocument();
    expect(input.value).toBe("Original Title");
  });

  it("commits on Enter and returns to display mode after a successful save", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn(() => Promise.resolve());
    const { rerender } = render(
      <InlineEditField id="t" label="Title" value="Old" onSave={onSave} />,
    );
    await user.click(screen.getByTestId("inline-edit-display-t"));
    const input = screen.getByTestId("inline-edit-input-t") as HTMLInputElement;
    // jsdom's selection model doesn't honor the focus-time select(),
    // so clear explicitly to mirror the "auto-selected on focus" UX.
    await user.clear(input);
    await user.type(input, "New Title{Enter}");

    await waitFor(() => expect(onSave).toHaveBeenCalledWith("New Title"));
    // After the parent updates `value`, the component returns to
    // display mode and reflects the new value.
    rerender(
      <InlineEditField id="t" label="Title" value="New Title" onSave={onSave} />,
    );
    await waitFor(() =>
      expect(screen.getByTestId("inline-edit-display-t")).toHaveTextContent(
        "New Title",
      ),
    );
  });

  it("cancels on Escape, restoring the prior value without calling onSave", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    render(
      <InlineEditField id="t" label="Title" value="Keep me" onSave={onSave} />,
    );
    await user.click(screen.getByTestId("inline-edit-display-t"));
    const input = screen.getByTestId("inline-edit-input-t") as HTMLInputElement;
    await user.clear(input);
    await user.type(input, "Throwaway");
    expect(input.value).toBe("Throwaway");
    fireEvent.keyDown(input, { key: "Escape" });
    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByTestId("inline-edit-display-t")).toHaveTextContent(
      "Keep me",
    );
  });

  it("returns to edit mode and keeps the draft when onSave throws", async () => {
    const user = userEvent.setup();
    let callCount = 0;
    const onSave = vi.fn(() => {
      callCount += 1;
      if (callCount === 1) {
        return Promise.reject(new Error("validation: display_name: blank"));
      }
      return Promise.resolve();
    });
    const { rerender } = render(
      <InlineEditField
        id="t"
        label="Title"
        value="Stable"
        onSave={onSave}
        errorMessage={null}
      />,
    );
    await user.click(screen.getByTestId("inline-edit-display-t"));
    const input = screen.getByTestId("inline-edit-input-t") as HTMLInputElement;
    await user.clear(input);
    await user.type(input, "Draft 1{Enter}");
    // After the rejection the component is back in editing mode with
    // the operator's draft preserved.
    await waitFor(() => {
      expect(screen.getByTestId("inline-edit-input-t")).toBeInTheDocument();
    });
    expect(
      (screen.getByTestId("inline-edit-input-t") as HTMLInputElement).value,
    ).toBe("Draft 1");

    // The parent surfaces the failure via errorMessage.
    rerender(
      <InlineEditField
        id="t"
        label="Title"
        value="Stable"
        onSave={onSave}
        errorMessage="display_name: blank"
      />,
    );
    expect(screen.getByTestId("inline-edit-error-t")).toHaveTextContent(
      "display_name: blank",
    );
  });

  it("treats an unchanged value as a quiet cancel — no onSave call", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    render(
      <InlineEditField id="t" label="Title" value="Unchanged" onSave={onSave} />,
    );
    await user.click(screen.getByTestId("inline-edit-display-t"));
    fireEvent.keyDown(screen.getByTestId("inline-edit-input-t"), {
      key: "Enter",
    });
    expect(onSave).not.toHaveBeenCalled();
  });
});

// ─────────────────────────────────────────────────────────────────────────
// StrategyDetailRoute — edit cycle keeps the route mounted.
//
// Tracking on `data-strategy-id` confirms the inline edit happens
// in place; a remount or router push would change identity on the
// element.

describe("StrategyDetailRoute (edit cycle stability)", () => {
  const STRATEGY_ID = "01J0DETAILROUTEID0000000001";

  beforeEach(() => {
    // Mock the global fetch with a tiny state machine: GET returns
    // the current strategy, PATCH mutates it in place. No popup,
    // no overlay — just an in-memory backend that mirrors the
    // route's contract.
    const state = {
      manifest: {
        id: STRATEGY_ID,
        display_name: "Pre-Edit Title",
        plain_summary: "Pre-Edit Summary",
        creator: "@op",
        template: "trend_follower",
        asset_universe: ["BTC/USD"],
        decision_cadence_minutes: 60,
      },
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = typeof input === "string" ? input : input.toString();
        const method = init?.method ?? "GET";
        if (url.startsWith("/api/strategy/") && method === "GET") {
          return new Response(JSON.stringify(state), {
            status: 200,
            headers: { "content-type": "application/json" },
          });
        }
        if (url.startsWith("/api/strategy/") && method === "PATCH") {
          const body = init?.body ? JSON.parse(init.body as string) : {};
          if (typeof body.display_name === "string") {
            state.manifest.display_name = body.display_name;
          }
          if (typeof body.plain_summary === "string") {
            state.manifest.plain_summary = body.plain_summary;
          }
          return new Response(JSON.stringify(state), {
            status: 200,
            headers: { "content-type": "application/json" },
          });
        }
        return new Response("not implemented", { status: 501 });
      }),
    );
  });

  function renderRoute() {
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    return render(
      <QueryClientProvider client={qc}>
        <MemoryRouter initialEntries={[`/strategies/${STRATEGY_ID}`]}>
          <Routes>
            <Route
              path="/strategies/:id"
              element={<StrategyDetailRoute />}
            />
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>,
    );
  }

  it("renders the strategy id and title from the backend", async () => {
    renderRoute();
    await waitFor(() =>
      expect(screen.getByTestId("strategy-detail-id")).toHaveTextContent(
        STRATEGY_ID,
      ),
    );
    await waitFor(() =>
      expect(
        screen.getByTestId("inline-edit-display-display-name"),
      ).toHaveTextContent("Pre-Edit Title"),
    );
  });

  it("keeps the detail view mounted across an edit cycle (no remount, no router push)", async () => {
    const user = userEvent.setup();
    renderRoute();

    // Wait for the initial render to land.
    await waitFor(() =>
      expect(screen.getByTestId("strategy-detail-id")).toHaveTextContent(
        STRATEGY_ID,
      ),
    );
    const viewBefore = screen.getByTestId("strategy-detail-view");
    expect(viewBefore.getAttribute("data-strategy-id")).toBe(STRATEGY_ID);

    // Drive an edit cycle.
    await user.click(
      screen.getByTestId("inline-edit-display-display-name"),
    );
    const input = screen.getByTestId(
      "inline-edit-input-display-name",
    ) as HTMLInputElement;
    await act(async () => {
      await user.clear(input);
      await user.type(input, "Post-Edit Title{Enter}");
    });

    // The display reflects the new value AND the same view node is
    // still mounted with the same data-strategy-id. A router push
    // or query-key change would have unmounted/remounted this node.
    await waitFor(() =>
      expect(
        screen.getByTestId("inline-edit-display-display-name"),
      ).toHaveTextContent("Post-Edit Title"),
    );
    const viewAfter = screen.getByTestId("strategy-detail-view");
    expect(viewAfter).toBe(viewBefore);
    expect(viewAfter.getAttribute("data-strategy-id")).toBe(STRATEGY_ID);
  });

  it("renders no popup / modal / sheet / popover after entering edit mode", async () => {
    const user = userEvent.setup();
    renderRoute();
    await waitFor(() =>
      expect(
        screen.getByTestId("inline-edit-display-display-name"),
      ).toHaveTextContent("Pre-Edit Title"),
    );
    await user.click(
      screen.getByTestId("inline-edit-display-display-name"),
    );
    // No focus-trap overlay primitives appear in the DOM. The
    // inline-edit affordance must be the only thing that mounted.
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(document.querySelector("[data-state='open'][role='dialog']")).toBeNull();
  });
});
