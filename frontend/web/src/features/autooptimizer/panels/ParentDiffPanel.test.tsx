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
});
