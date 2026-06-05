import { describe, expect, it, vi, afterEach } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { OptimizerHome } from "./OptimizerHome";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

describe("OptimizerHome", () => {
  it("renders the Optimizer header and the writers + recent-cycles panels", async () => {
    vi.spyOn(client, "apiFetch").mockImplementation(async (url: string) => {
      if (url.includes("/health")) return { status: "ok", probes: [] };
      return [];
    });
    renderWithProviders(<OptimizerHome />);
    expect(await screen.findByText("Experiment writers")).toBeInTheDocument();
    expect(screen.getByText("Recent cycles")).toBeInTheDocument();
  });
});
