import { afterEach, describe, expect, it, vi } from "vitest";
import * as client from "@/api/client";
import {
  listNanochatCheckpoints,
  getNanochatCheckpoint,
  approveNanochatCheckpoint,
  startAutoresearchRun,
  stopAutoresearchRun,
  listAutoresearchRuns,
  getAutoresearchRun,
  listAutoresearchExperiments,
  nanochatKeys,
  autoresearchKeys,
} from "@/api/nanochat";

afterEach(() => vi.restoreAllMocks());

describe("nanochat checkpoints", () => {
  it("listNanochatCheckpoints fetches /api/nanochat/checkpoints", async () => {
    const spy = vi.spyOn(client, "apiFetch").mockResolvedValue([]);
    await listNanochatCheckpoints();
    expect(spy).toHaveBeenCalledWith("/api/nanochat/checkpoints");
  });

  it("listNanochatCheckpoints forwards promoted_only filter", async () => {
    const spy = vi.spyOn(client, "apiFetch").mockResolvedValue([]);
    await listNanochatCheckpoints({ promoted_only: true });
    expect(spy).toHaveBeenCalledWith("/api/nanochat/checkpoints?promoted_only=true");
  });

  it("getNanochatCheckpoint fetches /api/nanochat/checkpoints/:model_id with URI encoding", async () => {
    const spy = vi.spyOn(client, "apiFetch").mockResolvedValue({});
    await getNanochatCheckpoint("mod el/1");
    expect(spy).toHaveBeenCalledWith("/api/nanochat/checkpoints/mod%20el%2F1");
  });

  it("approveNanochatCheckpoint POSTs to /api/nanochat/checkpoints/:model_id/approve", async () => {
    const spy = vi.spyOn(client, "apiFetch").mockResolvedValue({});
    await approveNanochatCheckpoint("mod-1");
    expect(spy).toHaveBeenCalledWith(
      "/api/nanochat/checkpoints/mod-1/approve",
      expect.objectContaining({ method: "POST" }),
    );
  });
});

describe("autoresearch runs", () => {
  it("startAutoresearchRun POSTs to /api/autoresearch/runs with body", async () => {
    const spy = vi.spyOn(client, "apiFetch").mockResolvedValue({ run_id: "r1" });
    await startAutoresearchRun({
      source_strategy_id: "strat-1",
      label_strategy: "price_forward",
      label_config: { price_forward_threshold: 0.01 },
      run_tag: "jun14a",
    });
    expect(spy).toHaveBeenCalledWith(
      "/api/autoresearch/runs",
      expect.objectContaining({
        method: "POST",
        body: expect.stringContaining("run_tag"),
      }),
    );
  });

  it("stopAutoresearchRun POSTs to /api/autoresearch/runs/:run_id/stop", async () => {
    const spy = vi.spyOn(client, "apiFetch").mockResolvedValue({});
    await stopAutoresearchRun("run-1");
    expect(spy).toHaveBeenCalledWith(
      "/api/autoresearch/runs/run-1/stop",
      expect.objectContaining({ method: "POST" }),
    );
  });

  it("listAutoresearchRuns fetches /api/autoresearch/runs", async () => {
    const spy = vi.spyOn(client, "apiFetch").mockResolvedValue([]);
    await listAutoresearchRuns();
    expect(spy).toHaveBeenCalledWith("/api/autoresearch/runs");
  });

  it("getAutoresearchRun fetches /api/autoresearch/runs/:run_id", async () => {
    const spy = vi.spyOn(client, "apiFetch").mockResolvedValue({});
    await getAutoresearchRun("run-abc");
    expect(spy).toHaveBeenCalledWith("/api/autoresearch/runs/run-abc");
  });

  it("listAutoresearchExperiments fetches /api/autoresearch/runs/:run_id/experiments", async () => {
    const spy = vi.spyOn(client, "apiFetch").mockResolvedValue([]);
    await listAutoresearchExperiments("run-abc");
    expect(spy).toHaveBeenCalledWith("/api/autoresearch/runs/run-abc/experiments");
  });
});

describe("query key factories", () => {
  it("nanochatKeys.checkpoints() produces a stable key", () => {
    expect(nanochatKeys.checkpoints({})).toEqual(["nanochat", "checkpoints", {}]);
  });

  it("autoresearchKeys.experiments(id) nests under the run key", () => {
    expect(autoresearchKeys.experiments("r1")).toEqual([
      "autoresearch", "runs", "r1", "experiments",
    ]);
  });
});
