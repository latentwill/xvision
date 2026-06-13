// WU9 — Frontend API tests for pine-library module.
//
// Tests:
// 1. getPineLibrary() calls GET /api/strategy/pine-library and returns items.
// 2. importLibraryEntry(id) calls POST /api/strategy/pine-library/{id}/import.
// 3. importLibraryEntry encodes special chars in the id.

import { afterEach, describe, expect, test, vi } from "vitest";
import { getPineLibrary, importLibraryEntry } from "./pine-library";

function mockJson(body: unknown) {
  return Promise.resolve({
    ok: true,
    status: 200,
    json: () => Promise.resolve(body),
  } as Response);
}

const MOCK_LIBRARY_RESPONSE = {
  items: [
    { id: "rsi-threshold", name: "RSI Threshold", description: "RSI fade strategy" },
    { id: "ma-crossover", name: "MA Crossover", description: "MA cross strategy" },
  ],
};

const MOCK_IMPORT_RESPONSE = {
  strategy: {
    manifest: {
      id: "strat_01",
      display_name: "RSI Threshold",
    },
  },
  fidelity_report: {
    captured: [],
    approximated: [],
    dropped: [],
    cost_model: {
      commission_type: "flat_bps",
      commission_value_bps: 5,
      slippage_model: "fixed_bps",
      slippage_value_bps: 3,
      fill_timing: "next_open",
      note: "",
    },
  },
};

describe("pine-library API", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  // ── 1. getPineLibrary → GET /api/strategy/pine-library ───────────────────

  test("getPineLibrary calls GET /api/strategy/pine-library", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() => mockJson(MOCK_LIBRARY_RESPONSE));

    const result = await getPineLibrary();

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/strategy/pine-library",
      expect.objectContaining({ headers: expect.any(Object) }),
    );
    expect(result.items).toHaveLength(2);
    expect(result.items[0].id).toBe("rsi-threshold");
    expect(result.items[1].name).toBe("MA Crossover");
  });

  // ── 2. importLibraryEntry → POST /api/strategy/pine-library/{id}/import ──

  test("importLibraryEntry calls POST /api/strategy/pine-library/{id}/import", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() => mockJson(MOCK_IMPORT_RESPONSE));

    const result = await importLibraryEntry("rsi-threshold");

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/strategy/pine-library/rsi-threshold/import",
      expect.objectContaining({ method: "POST" }),
    );
    expect(result.strategy.manifest.display_name).toBe("RSI Threshold");
    expect(result.fidelity_report).toBeDefined();
  });

  // ── 3. importLibraryEntry encodes special chars ───────────────────────────

  test("importLibraryEntry URL-encodes the id", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() => mockJson(MOCK_IMPORT_RESPONSE));

    await importLibraryEntry("id with spaces/and-slash");

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/strategy/pine-library/id%20with%20spaces%2Fand-slash/import",
      expect.objectContaining({ method: "POST" }),
    );
  });
});
