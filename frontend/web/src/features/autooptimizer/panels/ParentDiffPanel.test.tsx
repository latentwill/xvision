import { describe, expect, it, vi, afterEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { ParentDiffPanel } from "./ParentDiffPanel";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

describe("ParentDiffPanel", () => {
  it("shows a changed key with before/after values", async () => {
    vi.spyOn(client, "apiFetch").mockImplementation(async (url: string) => {
      if (url.includes("/blob/child")) return { entry_threshold: 0.7, name: "child" };
      if (url.includes("/blob/parent")) return { entry_threshold: 0.5, name: "parent" };
      return {};
    });
    renderWithProviders(
      <ParentDiffPanel childHash="child" parentHash="parent" />,
    );
    expect(await screen.findByText("What this experiment changed")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText("entry_threshold")).toBeInTheDocument());
    expect(screen.getByText("0.5")).toBeInTheDocument();
    expect(screen.getByText("0.7")).toBeInTheDocument();
  });

  it("renders AgentRef prompt changes by role instead of dumping raw agents JSON", async () => {
    vi.spyOn(client, "apiFetch").mockImplementation(async (url: string) => {
      if (url.includes("/blob/child")) {
        return {
          agents: [
            {
              agent_id: "01AGENT",
              role: "trader",
              prompt: "Trade with trend confirmation.",
            },
          ],
        };
      }
      if (url.includes("/blob/parent")) {
        return {
          agents: [{ agent_id: "01AGENT", role: "trader" }],
        };
      }
      return {};
    });

    renderWithProviders(
      <ParentDiffPanel childHash="child" parentHash="parent" />,
    );

    expect(await screen.findByText("agents.trader.prompt")).toBeInTheDocument();
    expect(screen.getByText("—")).toBeInTheDocument();
    expect(screen.getByText("Trade with trend confirmation.")).toBeInTheDocument();
    expect(screen.queryByText(/"agent_id":"01AGENT"/)).not.toBeInTheDocument();
  });

  it("left-aligns the heading, summary, and diff content", async () => {
    vi.spyOn(client, "apiFetch").mockImplementation(async (url: string) => {
      if (url.includes("/blob/child")) return { entry_threshold: 0.7, name: "child" };
      if (url.includes("/blob/parent")) return { entry_threshold: 0.5, name: "parent" };
      return {};
    });
    renderWithProviders(
      <ParentDiffPanel childHash="child" parentHash="parent" />,
    );

    const heading = await screen.findByRole("heading", {
      name: "What this experiment changed",
    });
    const panel = heading.closest("section");
    expect(panel?.className).toContain("text-left");
    expect(screen.getByText(/parent.*experiment/).className).toContain("m-0");
    expect((await screen.findByRole("table")).className).toContain("text-left");
  });
});
