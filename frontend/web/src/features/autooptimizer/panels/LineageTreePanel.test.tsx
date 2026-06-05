import { describe, expect, it, vi, afterEach } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { LineageTreePanel } from "./LineageTreePanel";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

describe("LineageTreePanel", () => {
  it("renders a parent and its child indented, each linking to the experiment screen", async () => {
    vi.spyOn(client, "apiFetch").mockResolvedValue([
      { bundle_hash: "parent00", parent_hash: null, gate_verdict: "Pass", status: "active", cycle_id: "cyc-1", created_at: "2026-06-01T00:00:00Z" },
      { bundle_hash: "child0001", parent_hash: "parent00", gate_verdict: "Fail", status: "rejected", cycle_id: "cyc-1", created_at: "2026-06-01T00:05:00Z" },
    ]);
    renderWithProviders(<LineageTreePanel cycleId="cyc-1" />);
    expect(await screen.findByText("Lineage tree")).toBeInTheDocument();
    const parentLink = await screen.findByRole("link", { name: /parent00/ });
    expect(parentLink).toHaveAttribute("href", "/optimizer/experiment/parent00");
    const childLink = await screen.findByRole("link", { name: /child0001/ });
    expect(childLink).toHaveAttribute("href", "/optimizer/experiment/child0001");
  });

  it("handles a self-parent node without infinite recursion", async () => {
    vi.spyOn(client, "apiFetch").mockResolvedValue([
      { bundle_hash: "selfref01", parent_hash: "selfref01", gate_verdict: "Pass", status: "active", cycle_id: "cyc-1", created_at: "2026-06-01T00:00:00Z" },
    ]);
    renderWithProviders(<LineageTreePanel cycleId="cyc-1" />);
    const links = await screen.findAllByRole("link", { name: /selfref01/ });
    expect(links).toHaveLength(1);
  });

  it("renders nodes even when parent_hash forms a cycle", async () => {
    vi.spyOn(client, "apiFetch").mockResolvedValue([
      { bundle_hash: "aaaa1111", parent_hash: "bbbb2222", gate_verdict: "Pass", status: "active", cycle_id: "cyc-1", created_at: "2026-06-01T00:00:00Z" },
      { bundle_hash: "bbbb2222", parent_hash: "aaaa1111", gate_verdict: "Fail", status: "rejected", cycle_id: "cyc-1", created_at: "2026-06-01T00:01:00Z" },
    ]);
    renderWithProviders(<LineageTreePanel cycleId="cyc-1" />);
    expect(await screen.findByRole("link", { name: /aaaa1111/ })).toBeInTheDocument();
    expect(await screen.findByRole("link", { name: /bbbb2222/ })).toBeInTheDocument();
    // Each node must appear exactly once
    expect(screen.getAllByRole("link", { name: /aaaa1111/ })).toHaveLength(1);
    expect(screen.getAllByRole("link", { name: /bbbb2222/ })).toHaveLength(1);
  });

  it("renders all nodes when a normal root and a disconnected a→b→a cycle coexist", async () => {
    // "root0001" is a normal root (no parent); "cycleaa1" and "cyclebb2" form a
    // pure a→b→a cycle with no entry point from the normal tree.  All three
    // nodes must render exactly once.
    vi.spyOn(client, "apiFetch").mockResolvedValue([
      { bundle_hash: "root0001", parent_hash: null, gate_verdict: "Pass", status: "active", cycle_id: "cyc-2", created_at: "2026-06-01T00:00:00Z" },
      { bundle_hash: "cycleaa1", parent_hash: "cyclebb2", gate_verdict: "Pass", status: "active", cycle_id: "cyc-2", created_at: "2026-06-01T00:01:00Z" },
      { bundle_hash: "cyclebb2", parent_hash: "cycleaa1", gate_verdict: "Fail", status: "rejected", cycle_id: "cyc-2", created_at: "2026-06-01T00:02:00Z" },
    ]);
    renderWithProviders(<LineageTreePanel cycleId="cyc-2" />);

    // All three hashes must appear as links
    const rootLink = await screen.findByRole("link", { name: /root0001/ });
    const cycleALink = await screen.findByRole("link", { name: /cycleaa1/ });
    const cycleBLink = await screen.findByRole("link", { name: /cyclebb2/ });

    expect(rootLink).toHaveAttribute("href", "/optimizer/experiment/root0001");
    expect(cycleALink).toHaveAttribute("href", "/optimizer/experiment/cycleaa1");
    expect(cycleBLink).toHaveAttribute("href", "/optimizer/experiment/cyclebb2");

    // Each must appear exactly once — no duplicates
    expect(screen.getAllByRole("link", { name: /root0001/ })).toHaveLength(1);
    expect(screen.getAllByRole("link", { name: /cycleaa1/ })).toHaveLength(1);
    expect(screen.getAllByRole("link", { name: /cyclebb2/ })).toHaveLength(1);

    // Verify unique hrefs cover all three hashes
    const allLinks = screen.getAllByRole("link");
    const hrefs = allLinks.map((l) => l.getAttribute("href") ?? "");
    expect(hrefs).toContain("/optimizer/experiment/root0001");
    expect(hrefs).toContain("/optimizer/experiment/cycleaa1");
    expect(hrefs).toContain("/optimizer/experiment/cyclebb2");
  });
});
