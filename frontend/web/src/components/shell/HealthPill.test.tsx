import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { getHealth } from "@/api/health";
import type { HealthReport } from "@/api/types.gen";
import { HealthPill } from "./HealthPill";

vi.mock("@/api/health", async () => {
  const actual = await vi.importActual<typeof import("@/api/health")>(
    "@/api/health",
  );
  return {
    ...actual,
    getHealth: vi.fn(),
  };
});

function renderPill() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <HealthPill />
    </QueryClientProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("HealthPill", () => {
  it("renders pending and healthy states", async () => {
    let resolveHealth!: (report: HealthReport) => void;
    vi.mocked(getHealth).mockReturnValue(
      new Promise((resolve) => {
        resolveHealth = resolve;
      }),
    );

    renderPill();
    expect(screen.getByText("checking…")).toBeInTheDocument();

    resolveHealth(report("ok"));
    expect(await screen.findByText("engine ok")).toBeInTheDocument();
  });

  it("renders degraded and down states", async () => {
    vi.mocked(getHealth).mockResolvedValue(report("degraded"));
    const first = renderPill();
    expect(await screen.findByText("degraded")).toBeInTheDocument();

    first.unmount();
    cleanup();
    vi.mocked(getHealth).mockResolvedValue(report("down"));
    renderPill();
    expect(await screen.findByText("engine down")).toBeInTheDocument();
  });

  it("renders offline when health fetch fails", async () => {
    vi.mocked(getHealth).mockRejectedValue(new Error("offline"));
    renderPill();
    expect(await screen.findByText("offline")).toBeInTheDocument();
  });

  it("builds a probe summary title", async () => {
    vi.mocked(getHealth).mockResolvedValue(report("degraded"));
    renderPill();

    const pill = await screen.findByTitle(/engine: degraded/);
    expect(pill).toHaveAttribute("title", expect.stringContaining("42ms"));
  });
});

function report(status: HealthReport["status"]): HealthReport {
  return {
    status,
    probes: [
      {
        name: "engine",
        status,
        detail: status === "ok" ? null : "slow",
        latency_ms: 42,
      },
    ],
  };
}
