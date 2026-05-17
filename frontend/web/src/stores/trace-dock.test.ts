// frontend/web/src/stores/trace-dock.test.ts
import { beforeEach, describe, expect, test } from "vitest";
import { useTraceDock } from "./trace-dock";

describe("trace-dock store", () => {
  beforeEach(() => {
    useTraceDock.setState({
      height: "collapsed",
      selectedSpanId: null,
      activeRunId: null,
      mode: "post-hoc",
    });
  });

  test("toggle: collapsed → working → collapsed", () => {
    expect(useTraceDock.getState().height).toBe("collapsed");
    useTraceDock.getState().toggle();
    expect(useTraceDock.getState().height).toBe("working");
    useTraceDock.getState().toggle();
    expect(useTraceDock.getState().height).toBe("collapsed");
  });

  test("setHeight respects all four states", () => {
    const heights = ["collapsed", "peek", "working", "full"] as const;
    for (const h of heights) {
      useTraceDock.getState().setHeight(h);
      expect(useTraceDock.getState().height).toBe(h);
    }
  });

  test("setActiveRun resets selectedSpan", () => {
    useTraceDock.setState({ selectedSpanId: "s5" });
    useTraceDock.getState().setActiveRun("run_other", "post-hoc");
    expect(useTraceDock.getState().selectedSpanId).toBeNull();
    expect(useTraceDock.getState().activeRunId).toBe("run_other");
    expect(useTraceDock.getState().mode).toBe("post-hoc");
  });
});
