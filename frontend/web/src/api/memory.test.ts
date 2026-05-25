import { afterEach, describe, expect, test, vi } from "vitest";

import {
  activatePattern,
  demotePattern,
  listMemory,
  listMemoryNamespaces,
} from "./memory";

function mockJson(body: unknown) {
  return Promise.resolve({
    ok: true,
    status: 200,
    json: () => Promise.resolve(body),
  } as Response);
}

describe("memory API", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  test("listMemory includes lifecycle filters", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() => mockJson({ items: [], total: 0 }));

    await listMemory({
      tier: "pattern",
      namespace: "global",
      promotion_state: "staged",
      forgotten_only: true,
      limit: 10,
    });

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/memory?tier=pattern&namespace=global&promotion_state=staged&limit=10&forgotten_only=true",
      expect.any(Object),
    );
  });

  test("activatePattern and demotePattern encode ids", async () => {
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      mockJson({
        id: "pat/1",
        namespace: "global",
        tier: "pattern",
        text: "p",
        created_at: "2026-05-25T00:00:00Z",
        run_id: null,
        scenario_id: null,
        cycle_idx: null,
        training_window_end: "2026-05-24T00:00:00Z",
      }),
    );

    await activatePattern("pat/1");
    await demotePattern("pat/1");

    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "/api/memory/pat%2F1/activate",
      expect.objectContaining({ method: "POST" }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "/api/memory/pat%2F1/demote",
      expect.objectContaining({ method: "POST" }),
    );
  });

  test("listMemoryNamespaces calls the namespace inventory route", async () => {
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      mockJson({
        items: [
          {
            namespace: "agent:A",
            live_total: 3,
            observations: 2,
            active_patterns: 1,
            staged_patterns: 0,
            forgotten: 1,
            latest_created_at: "2026-05-25T00:00:00Z",
          },
        ],
        total: 1,
      }),
    );

    const out = await listMemoryNamespaces();

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/memory/namespaces",
      expect.any(Object),
    );
    expect(out.items[0].namespace).toBe("agent:A");
    expect(out.items[0].forgotten).toBe(1);
  });
});
