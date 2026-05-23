/**
 * Tests for B3 primitives + the kline-anchor adapter math + the
 * AIAnnotationDashboard surface filter helper.
 *
 * Canvas-heavy bits (KlineCandlePane mount) are NOT exercised here;
 * the chart-lab/dashboards/annotated route covers manual visual review.
 */
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";

import {
  DEFAULT_BOUNDS,
  deriveRange,
  xForIndex,
  yForPrice,
  type AnchorBounds,
} from "../adapters/kline-anchor";

import { Callout, CALLOUT_WIDTH } from "./Callout";
import { spreadRowXs } from "./AnnotationOverlay";
import { InsightLog } from "./InsightLog";
import { AiEnginePill } from "./AiEnginePill";
import { typesForFilter } from "../surfaces/AIAnnotationDashboard";
import type { Annotation } from "../types";

const SAMPLE: Annotation = {
  idx: 22,
  side: "top",
  type: "PATTERN",
  title: "Bull Flag",
  body: "test body",
  conf: 0.74,
  action: "WATCH",
};

const DANGER: Annotation = { ...SAMPLE, idx: 80, type: "RISK", danger: true, title: "Liquidation Wall" };

describe("kline-anchor: xForIndex", () => {
  const b: AnchorBounds = { ...DEFAULT_BOUNDS, width: 1000, height: 400 };
  it("returns the middle of usable when count = 1", () => {
    const usable = b.width - b.padLeft - b.padRight;
    expect(xForIndex(0, 1, b)).toBeCloseTo(b.padLeft + usable / 2, 4);
  });
  it("linearly distributes across usable width for count > 1", () => {
    // 11 candles → usable / 10 steps
    expect(xForIndex(0, 11, b)).toBeCloseTo(b.padLeft, 4);
    const usable = b.width - b.padLeft - b.padRight;
    expect(xForIndex(10, 11, b)).toBeCloseTo(b.padLeft + usable, 4);
    expect(xForIndex(5, 11, b)).toBeCloseTo(b.padLeft + usable * 0.5, 4);
  });
  it("returns NaN for out-of-range / empty inputs", () => {
    expect(Number.isNaN(xForIndex(-1, 10, b))).toBe(true);
    expect(Number.isNaN(xForIndex(10, 10, b))).toBe(true);
    expect(Number.isNaN(xForIndex(0, 0, b))).toBe(true);
  });
});

describe("kline-anchor: yForPrice", () => {
  const b: AnchorBounds = { ...DEFAULT_BOUNDS, width: 1000, height: 400 };
  it("higher prices map to smaller y (higher on screen)", () => {
    const r = { min: 0, max: 100 };
    const top = yForPrice(100, r, b);
    const bot = yForPrice(0, r, b);
    expect(top).toBeLessThan(bot);
  });
  it("midpoint price maps to vertical midpoint of usable region", () => {
    const r = { min: 0, max: 100 };
    const usable = b.height - b.padTop - b.padBottom;
    expect(yForPrice(50, r, b)).toBeCloseTo(b.padTop + usable / 2, 4);
  });
  it("returns NaN for a zero-span range", () => {
    expect(Number.isNaN(yForPrice(10, { min: 10, max: 10 }, b))).toBe(true);
  });
});

describe("kline-anchor: deriveRange", () => {
  it("includes 4% padding above the max and below the min", () => {
    const r = deriveRange([100, 110, 120], [80, 90, 85]);
    expect(r.min).toBeLessThan(80);
    expect(r.max).toBeGreaterThan(120);
    expect(r.max - r.min).toBeCloseTo((120 - 80) * 1.08, 5);
  });
  it("returns a placeholder range for empty input", () => {
    expect(deriveRange([], [])).toEqual({ min: 0, max: 1 });
  });
});

describe("spreadRowXs", () => {
  const b: AnchorBounds = { ...DEFAULT_BOUNDS, width: 1200, height: 480 };
  it("returns [] for zero count", () => {
    expect(spreadRowXs(0, b, CALLOUT_WIDTH)).toEqual([]);
  });
  it("centers a single callout in the usable width", () => {
    const xs = spreadRowXs(1, b, CALLOUT_WIDTH);
    const usable = b.width - b.padLeft - b.padRight - CALLOUT_WIDTH;
    expect(xs).toHaveLength(1);
    expect(xs[0]).toBeCloseTo(b.padLeft + usable / 2, 4);
  });
  it("evenly steps n>1 callouts across the usable width", () => {
    const xs = spreadRowXs(3, b, CALLOUT_WIDTH);
    expect(xs).toHaveLength(3);
    expect(xs[0]).toBeCloseTo(b.padLeft, 4);
    const last = b.width - b.padLeft - b.padRight - CALLOUT_WIDTH;
    expect(xs[2]).toBeCloseTo(b.padLeft + last, 4);
    expect(xs[1] - xs[0]).toBeCloseTo(xs[2] - xs[1], 4);
  });
});

describe("Callout", () => {
  it("renders annotation chrome (type, title, body, idx, action)", () => {
    render(<Callout annotation={SAMPLE} />);
    expect(screen.getByText("PATTERN")).toBeInTheDocument();
    expect(screen.getByText("Bull Flag")).toBeInTheDocument();
    expect(screen.getByText("test body")).toBeInTheDocument();
    expect(screen.getByText("idx · 22")).toBeInTheDocument();
    expect(screen.getByText("WATCH")).toBeInTheDocument();
  });
  it("uses red accent for danger annotations", () => {
    const { container } = render(<Callout annotation={DANGER} />);
    const card = container.firstChild as HTMLElement;
    // Card border switches to the danger rgba; we assert the inline style.
    // CSSOM normalises rgba() with spaces; match the canonical form.
    expect(card.style.border).toMatch(/rgba\(200,\s*68,\s*58/);
  });
});

describe("InsightLog", () => {
  it("renders expanded list with N events label", () => {
    render(
      <InsightLog
        annotations={[SAMPLE, DANGER]}
        open={true}
        onToggle={() => {}}
      />,
    );
    expect(screen.getByTestId("insight-log-open")).toBeInTheDocument();
    expect(screen.getByText("Insight Log · 2 events")).toBeInTheDocument();
  });
  it("renders collapsed rail when open=false", () => {
    render(
      <InsightLog
        annotations={[SAMPLE, DANGER]}
        open={false}
        onToggle={() => {}}
      />,
    );
    expect(screen.getByTestId("insight-log-collapsed")).toBeInTheDocument();
    expect(screen.queryByTestId("insight-log-open")).toBeNull();
  });
  it("filters by visibleTypes", () => {
    render(
      <InsightLog
        annotations={[SAMPLE, DANGER]}
        visibleTypes={new Set(["RISK"])}
        open={true}
        onToggle={() => {}}
      />,
    );
    expect(screen.getByText("Liquidation Wall")).toBeInTheDocument();
    expect(screen.queryByText("Bull Flag")).toBeNull();
  });
});

describe("AiEnginePill", () => {
  it("renders the label", () => {
    render(<AiEnginePill />);
    expect(screen.getByText("AI Engine · live")).toBeInTheDocument();
  });
  it("registers the xvnAiPulse keyframe in document.head exactly once", () => {
    render(
      <>
        <AiEnginePill />
        <AiEnginePill />
      </>,
    );
    expect(document.head.querySelectorAll("#xvn-ai-pulse-keyframe").length).toBe(1);
  });
});

describe("AIAnnotationDashboard.typesForFilter", () => {
  it("ALL returns undefined (no filter)", () => {
    expect(typesForFilter("ALL")).toBeUndefined();
  });
  it("PATTERN includes structural callouts too", () => {
    const set = typesForFilter("PATTERN");
    expect(set!.has("PATTERN")).toBe(true);
    expect(set!.has("STRUCTURE")).toBe(true);
    expect(set!.has("RISK")).toBe(false);
  });
  it("FLOW pairs with REVERSION", () => {
    const set = typesForFilter("FLOW");
    expect(set!.has("FLOW")).toBe(true);
    expect(set!.has("REVERSION")).toBe(true);
    expect(set!.has("RISK")).toBe(false);
  });
  it("RISK is exclusively RISK", () => {
    const set = typesForFilter("RISK");
    expect([...set!]).toEqual(["RISK"]);
  });
});
