/**
 * Tests for the URL-synced roster hook + its pure helpers.
 */
import { describe, it, expect } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { MemoryRouter, Routes, Route, useLocation } from "react-router-dom";
import { type ReactNode } from "react";

import {
  addId,
  removeId,
  toggleId,
  setLeadId,
  parseRosterParam,
  serializeRosterParam,
  useChart2Roster,
} from "./useChart2Roster";

const AVAILABLE = ["fib", "ema", "brk", "msw", "mvr", "vsc", "lqh", "btc"];

describe("pure helpers", () => {
  describe("addId", () => {
    it("appends an id not already present", () => {
      expect(addId(["fib", "ema"], "brk", AVAILABLE)).toEqual(["fib", "ema", "brk"]);
    });
    it("is a no-op if the id is already selected", () => {
      const current = ["fib", "ema"];
      expect(addId(current, "fib", AVAILABLE)).toBe(current);
    });
    it("rejects ids outside `available`", () => {
      const current = ["fib"];
      expect(addId(current, "unknown", AVAILABLE)).toBe(current);
    });
  });

  describe("removeId", () => {
    it("removes when above min", () => {
      expect(removeId(["fib", "ema", "brk"], "ema", 2)).toEqual(["fib", "brk"]);
    });
    it("is a no-op at min", () => {
      const current = ["fib", "ema"];
      expect(removeId(current, "fib", 2)).toBe(current);
    });
    it("is a no-op when id is not selected", () => {
      const current = ["fib", "ema"];
      expect(removeId(current, "unknown", 2)).toBe(current);
    });
  });

  describe("toggleId", () => {
    it("adds when missing", () => {
      expect(toggleId(["fib", "ema"], "brk", AVAILABLE, 2)).toEqual(["fib", "ema", "brk"]);
    });
    it("removes when present, above min", () => {
      expect(toggleId(["fib", "ema", "brk"], "brk", AVAILABLE, 2)).toEqual(["fib", "ema"]);
    });
    it("won't remove when at min", () => {
      const current = ["fib", "ema"];
      expect(toggleId(current, "fib", AVAILABLE, 2)).toBe(current);
    });
  });

  describe("setLeadId", () => {
    it("moves an already-present id to the front", () => {
      expect(setLeadId(["fib", "ema", "brk"], "brk", AVAILABLE)).toEqual(["brk", "fib", "ema"]);
    });
    it("adds a missing id at the front", () => {
      expect(setLeadId(["fib", "ema"], "msw", AVAILABLE)).toEqual(["msw", "fib", "ema"]);
    });
    it("rejects ids outside `available`", () => {
      const current = ["fib"];
      expect(setLeadId(current, "unknown", AVAILABLE)).toBe(current);
    });
  });

  describe("parseRosterParam", () => {
    it("returns [] for null / empty", () => {
      expect(parseRosterParam(null, AVAILABLE)).toEqual([]);
      expect(parseRosterParam("", AVAILABLE)).toEqual([]);
    });
    it("trims, dedupes, and filters to available, preserving order", () => {
      expect(parseRosterParam(" fib , ema , fib , unknown , brk ", AVAILABLE)).toEqual([
        "fib",
        "ema",
        "brk",
      ]);
    });
  });

  describe("serializeRosterParam", () => {
    it("comma-joins", () => {
      expect(serializeRosterParam(["fib", "ema"])).toBe("fib,ema");
    });
    it("empty array → empty string", () => {
      expect(serializeRosterParam([])).toBe("");
    });
  });
});

// ── Hook ─────────────────────────────────────────────────────────────────

function wrap(initialUrl: string) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <MemoryRouter initialEntries={[initialUrl]}>
        <Routes>
          <Route path="*" element={<>{children}</>} />
        </Routes>
      </MemoryRouter>
    );
  };
}

describe("useChart2Roster", () => {
  it("falls back to defaultSelected when URL is empty", () => {
    const { result } = renderHook(
      () => useChart2Roster({ available: AVAILABLE, defaultSelected: ["fib", "ema", "brk"] }),
      { wrapper: wrap("/") },
    );
    expect(result.current.selectedIds).toEqual(["fib", "ema", "brk"]);
    expect(result.current.count).toBe(3);
  });

  it("reads selection from ?ids=...", () => {
    const { result } = renderHook(
      () => useChart2Roster({ available: AVAILABLE }),
      { wrapper: wrap("/?ids=ema,brk,msw") },
    );
    expect(result.current.selectedIds).toEqual(["ema", "brk", "msw"]);
  });

  it("falls back to default when URL has fewer than min ids", () => {
    const { result } = renderHook(
      () =>
        useChart2Roster({
          available: AVAILABLE,
          defaultSelected: ["fib", "ema"],
          min: 2,
        }),
      { wrapper: wrap("/?ids=ema") },
    );
    expect(result.current.selectedIds).toEqual(["fib", "ema"]);
  });

  it("add / remove / toggle / setLead update the URL", () => {
    let currentSearch = "";
    function Probe() {
      currentSearch = useLocation().search;
      return null;
    }
    const wrapper = ({ children }: { children: ReactNode }) => (
      <MemoryRouter initialEntries={["/?ids=fib,ema"]}>
        <Probe />
        <Routes>
          <Route path="*" element={<>{children}</>} />
        </Routes>
      </MemoryRouter>
    );

    const { result } = renderHook(
      () => useChart2Roster({ available: AVAILABLE }),
      { wrapper },
    );

    expect(result.current.selectedIds).toEqual(["fib", "ema"]);

    act(() => result.current.add("brk"));
    expect(currentSearch).toBe("?ids=fib%2Cema%2Cbrk");

    act(() => result.current.setLead("brk"));
    expect(currentSearch).toBe("?ids=brk%2Cfib%2Cema");

    act(() => result.current.remove("fib"));
    expect(currentSearch).toBe("?ids=brk%2Cema");

    // canRemove gates the × button at min selection
    expect(result.current.canRemove("brk")).toBe(false);
    expect(result.current.canRemove("ema")).toBe(false);

    act(() => result.current.toggle("msw"));
    expect(currentSearch).toBe("?ids=brk%2Cema%2Cmsw");
    expect(result.current.canRemove("brk")).toBe(true);
  });
});
