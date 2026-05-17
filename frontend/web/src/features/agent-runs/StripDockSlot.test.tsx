// frontend/web/src/features/agent-runs/StripDockSlot.test.tsx
import { afterEach, beforeEach, describe, expect, test } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { StripDockSlot } from "./StripDockSlot";
import { useTraceDock } from "@/stores/trace-dock";

function renderSlot() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <StripDockSlot />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("StripDockSlot", () => {
  beforeEach(() => {
    useTraceDock.setState({
      height: "collapsed",
      selectedSpanId: null,
      activeRunId: null,
      mode: "post-hoc",
      lastOpenHeight: "working",
    });
  });
  afterEach(() => cleanup());

  test("renders nothing when activeRunId is null", () => {
    renderSlot();
    expect(screen.queryByTestId("run-status-strip")).toBeNull();
    expect(screen.queryByTestId("trace-dock")).toBeNull();
  });

  test("renders RunStatusStrip when activeRunId set and height=collapsed", async () => {
    useTraceDock.setState({ activeRunId: "run_abc1234", height: "collapsed" });
    renderSlot();
    await waitFor(() => expect(screen.getByTestId("run-status-strip")).toBeInTheDocument());
  });

  test("renders TraceDock when height is non-collapsed", async () => {
    useTraceDock.setState({ activeRunId: "run_abc1234", height: "working" });
    renderSlot();
    await waitFor(() => expect(screen.getByTestId("trace-dock")).toBeInTheDocument());
    expect(screen.queryByTestId("run-status-strip")).toBeNull();
  });
});
