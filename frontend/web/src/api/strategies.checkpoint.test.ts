// Tests for patchAgentCheckpoint (s3ph.27 — persist AgentRef.checkpoint/veto
// end-to-end). Mirrors the pattern in nanochat.test.ts: spy on apiFetch, call
// the exported function, assert URL + method + body.

import { afterEach, describe, expect, it, vi } from "vitest";
import * as client from "@/api/client";
import { patchAgentCheckpoint } from "@/api/strategies";

afterEach(() => vi.restoreAllMocks());

describe("patchAgentCheckpoint", () => {
  it("PUTs to the correct URL with strategy id and role URI-encoded", async () => {
    const spy = vi.spyOn(client, "apiFetch").mockResolvedValue({} as never);
    await patchAgentCheckpoint("strat-1", "filter", {
      checkpoint: { model_id: "mod-approved" },
      veto: true,
    });
    expect(spy).toHaveBeenCalledWith(
      "/api/strategy/strat-1/agents/filter/checkpoint",
      expect.objectContaining({ method: "PUT" }),
    );
  });

  it("URI-encodes strategy id and role containing special characters", async () => {
    const spy = vi.spyOn(client, "apiFetch").mockResolvedValue({} as never);
    await patchAgentCheckpoint("strat/1", "role name", {
      checkpoint: null,
      veto: null,
    });
    expect(spy).toHaveBeenCalledWith(
      "/api/strategy/strat%2F1/agents/role%20name/checkpoint",
      expect.anything(),
    );
  });

  it("sends the checkpoint and veto fields in the JSON body", async () => {
    const spy = vi.spyOn(client, "apiFetch").mockResolvedValue({} as never);
    await patchAgentCheckpoint("strat-2", "trader", {
      checkpoint: { model_id: "mod-xyz" },
      veto: false,
    });
    const [, options] = spy.mock.calls[0]!;
    const body = JSON.parse((options as RequestInit).body as string);
    expect(body).toEqual({ checkpoint: { model_id: "mod-xyz" }, veto: false });
  });

  it("sends null checkpoint and null veto to clear the selection", async () => {
    const spy = vi.spyOn(client, "apiFetch").mockResolvedValue({} as never);
    await patchAgentCheckpoint("strat-3", "filter", {
      checkpoint: null,
      veto: null,
    });
    const [, options] = spy.mock.calls[0]!;
    const body = JSON.parse((options as RequestInit).body as string);
    expect(body).toEqual({ checkpoint: null, veto: null });
  });
});
