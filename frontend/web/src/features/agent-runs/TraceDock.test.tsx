// frontend/web/src/features/agent-runs/TraceDock.test.tsx
import { beforeEach, describe, expect, test } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { TraceDock } from "./TraceDock";
import { useTraceDock } from "@/stores/trace-dock";

function renderDock() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <TraceDock />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("TraceDock", () => {
  beforeEach(() => {
    useTraceDock.setState({
      height: "collapsed",
      selectedSpanId: null,
      activeRunId: null,
      lastOpenHeight: "working",
    });
  });

  test("renders nothing when activeRunId is null", () => {
    renderDock();
    expect(screen.queryByTestId("trace-dock")).toBeNull();
  });

  test("renders header when activeRunId set, hidden body when collapsed", async () => {
    useTraceDock.setState({ activeRunId: "run_abc1234", height: "collapsed" });
    renderDock();
    // Header still hidden at collapsed — strip handles that.
    expect(screen.queryByTestId("trace-dock-body")).toBeNull();
  });

  test("shows body at working height", async () => {
    useTraceDock.setState({ activeRunId: "run_abc1234", height: "working" });
    renderDock();
    expect(await screen.findByTestId("trace-dock-body")).toBeInTheDocument();
  });

  test("minimize button collapses the dock", async () => {
    useTraceDock.setState({ activeRunId: "run_abc1234", height: "working" });
    renderDock();
    await screen.findByTestId("trace-dock-body");
    await userEvent.click(screen.getByLabelText(/minimize/i));
    expect(useTraceDock.getState().height).toBe("collapsed");
  });

  test("renders no Full preset button (resize handle owns height)", async () => {
    useTraceDock.setState({ activeRunId: "run_abc1234", height: "working" });
    renderDock();
    await screen.findByTestId("trace-dock-body");
    expect(screen.queryByRole("button", { name: /^full$/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /^peek$/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /^working$/i })).toBeNull();
    // The pop-out arrows remain — the sole fullscreen affordance.
    expect(
      screen.getByLabelText(/pop out to dedicated view/i),
    ).toBeInTheDocument();
    // The resize handle is mounted.
    expect(screen.getByTestId("trace-dock-resize-handle")).toBeInTheDocument();
  });

  test("dock renders at the store's heightPx", async () => {
    useTraceDock.setState({
      activeRunId: "run_abc1234",
      height: "working",
      heightPx: 612,
    });
    renderDock();
    const dock = await screen.findByTestId("trace-dock");
    expect(dock).toHaveStyle({ height: "612px" });
  });

  test("inspector selection falls back to the first filtered span", async () => {
    useTraceDock.setState({ activeRunId: "run_abc1234", height: "working" });
    renderDock();
    await screen.findByTestId("trace-dock-body");
    await screen.findByTestId("flame-bar-s1");

    await userEvent.click(screen.getByRole("button", { name: /^MODEL$/i }));

    expect(screen.queryByText("s1")).not.toBeInTheDocument();
    expect(await screen.findByText("s3")).toBeInTheDocument();
  });

  test("Trade button (F-7) renders disabled when no broker.call spans are present", async () => {
    // MOCK_RUN_COMPLETED has no broker.call span — the affordance still
    // appears so the operator knows the concept exists, but a click is
    // a no-op. The disabled state surfaces the *reason* via title.
    useTraceDock.setState({ activeRunId: "run_abc1234", height: "working" });
    renderDock();
    await screen.findByTestId("trace-dock-body");
    const btn = screen.getByTestId("trace-dock-trade-button");
    expect(btn).toBeDisabled();
    expect(btn).toHaveAttribute(
      "title",
      expect.stringContaining("No broker.call spans"),
    );
  });
});
