import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { createCliJob } from "@/api/cli";
import { LiveCycleView } from "./LiveCycleView";

vi.mock("@/api/cli", () => ({
  createCliJob: vi.fn(),
}));

type Listener = (event: MessageEvent) => void;

class MockEventSource {
  static instances: MockEventSource[] = [];

  readonly url: string;
  private listeners = new Map<string, Set<Listener>>();

  constructor(url: string) {
    this.url = url;
    MockEventSource.instances.push(this);
  }

  addEventListener(type: string, listener: Listener) {
    const existing = this.listeners.get(type) ?? new Set<Listener>();
    existing.add(listener);
    this.listeners.set(type, existing);
  }

  removeEventListener(type: string, listener: Listener) {
    this.listeners.get(type)?.delete(listener);
  }

  close() {}

  emit(type: string, data: string) {
    for (const listener of this.listeners.get(type) ?? []) {
      listener(new MessageEvent(type, { data }));
    }
  }
}

beforeEach(() => {
  vi.resetAllMocks();
  MockEventSource.instances = [];
  vi.mocked(createCliJob).mockResolvedValue({
    job_id: "job-1",
    status: "queued",
  });
  Object.defineProperty(globalThis, "EventSource", {
    configurable: true,
    writable: true,
    value: MockEventSource,
  });
  Object.defineProperty(window, "EventSource", {
    configurable: true,
    writable: true,
    value: MockEventSource,
  });
});

describe("LiveCycleView", () => {
  it("renders named cycle events from the optimizer SSE stream", async () => {
    render(<LiveCycleView />);

    expect(screen.getByText(/Waiting for cycle/)).toBeInTheDocument();
    expect(MockEventSource.instances[0]?.url).toBe("/api/autooptimizer/events");

    MockEventSource.instances[0].emit(
      "cycle_started",
      JSON.stringify({
        kind: "cycle_started",
        display_label: "Cycle started",
        data: { cycle_id: "cycle-1" },
      }),
    );

    expect(await screen.findByRole("log")).toBeInTheDocument();
    expect(screen.getByText("Cycle started")).toBeInTheDocument();
    expect(screen.getByText("cycle-1")).toBeInTheDocument();
  });

  it("launches evening cycle through CLI jobs and renders stdout cycle events", async () => {
    const user = userEvent.setup();
    render(<LiveCycleView />);

    await user.type(screen.getByPlaceholderText("Strategy ID"), "example-trend-follower");
    await user.type(screen.getByPlaceholderText("5.00"), "2.50");
    await user.click(screen.getByRole("button", { name: "Start evening run" }));

    await waitFor(() => expect(createCliJob).toHaveBeenCalledTimes(1));
    const request = vi.mocked(createCliJob).mock.calls[0][0];
    expect(request.timeout_secs).toBe(3600);
    expect(request.argv).toEqual(
      expect.arrayContaining([
        "optimizer",
        "evening-cycle",
        "--mock",
        "--strategy",
        "example-trend-follower",
        "--budget",
        "2.5",
      ]),
    );
    expect(request.argv[2]).toBe("--session-id");
    expect(request.argv[3]).toMatch(/^ui-/);

    const jobStream = MockEventSource.instances.find((source) =>
      source.url.includes("/api/cli/jobs/job-1/events"),
    );
    expect(jobStream).toBeDefined();

    jobStream!.emit(
      "stdout_chunk",
      JSON.stringify({
        chunk:
          '{"type":"mutation_gated","cycle_id":"cycle-2","child_hash":"child-1","passed":false}\n',
      }),
    );
    expect(await screen.findByText("Gate evaluated")).toBeInTheDocument();
    expect(screen.getByText("cycle-2")).toBeInTheDocument();

    jobStream!.emit("job_finished", JSON.stringify({ status: "succeeded" }));
    expect(await screen.findByText("Optimizer job finished")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start evening run" })).toBeEnabled();
  });

  it("can launch a non-mock evening cycle from the UI", async () => {
    const user = userEvent.setup();
    render(<LiveCycleView />);

    await user.type(screen.getByPlaceholderText("Strategy ID"), "example-trend-follower");
    await user.click(screen.getByLabelText("Mock"));
    await user.click(screen.getByRole("button", { name: "Start evening run" }));

    await waitFor(() => expect(createCliJob).toHaveBeenCalledTimes(1));
    const request = vi.mocked(createCliJob).mock.calls[0][0];
    expect(request.argv).toEqual(expect.arrayContaining(["optimizer", "evening-cycle"]));
    expect(request.argv).toEqual(expect.arrayContaining(["--strategy", "example-trend-follower"]));
    expect(request.argv).not.toContain("--mock");
  });
});
