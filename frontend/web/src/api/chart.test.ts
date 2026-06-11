// frontend/web/src/api/chart.test.ts
import { describe, expect, it } from "vitest";
import { chartKeys, runChartIncludeKey, runChartPath } from "./chart";

describe("runChartIncludeKey", () => {
  it("is empty for the full payload", () => {
    expect(runChartIncludeKey(undefined)).toBe("");
    expect(runChartIncludeKey([])).toBe("");
  });

  it("sorts tokens canonically so key order never splits the cache", () => {
    expect(runChartIncludeKey(["baseline", "equity"])).toBe("baseline,equity");
    expect(runChartIncludeKey(["equity", "baseline"])).toBe("baseline,equity");
  });
});

describe("chartKeys.run", () => {
  it("locks the key shape [chart, run, id, includeKey]", () => {
    expect(chartKeys.run("r1")).toEqual(["chart", "run", "r1", ""]);
    expect(chartKeys.run("r1", ["equity"])).toEqual(["chart", "run", "r1", "equity"]);
    expect(chartKeys.run("r1", ["markers", "bars"])).toEqual([
      "chart", "run", "r1", "bars,markers",
    ]);
  });
});

describe("runChartPath", () => {
  it("omits the query for the full payload", () => {
    expect(runChartPath("r1")).toBe("/api/eval/runs/r1/chart");
  });

  it("appends a canonical include param", () => {
    expect(runChartPath("r 1", ["equity", "baseline"])).toBe(
      "/api/eval/runs/r%201/chart?include=baseline%2Cequity",
    );
  });
});
