// frontend/web/src/features/agent-runs/render-parity.test.tsx
//
// WS-8 Part 1 — RENDER PARITY. The headline guarantee: every event the trace
// can carry RENDERS, and nothing is silently dropped.
//
// Two row vocabularies flow through the dock:
//   - observability *spans* (`SpanKind`), and
//   - bar-level *engine events* (`EngineEvent.kind`), projected onto
//     `engine.event` spans by `api/agent-runs.ts`.
//
// This test enumerates EVERY known kind of both, renders it through the real
// `SpanTree`, and asserts each gets a non-fallback badge + a row. It then
// renders an UNKNOWN span kind and an UNKNOWN engine kind and asserts they
// render the TYPED FALLBACK row (badge + raw kind) rather than vanishing —
// the never-go-dark contract.

import { describe, expect, test } from "vitest";
import { render, screen, within } from "@testing-library/react";
import type { RunSpan, SpanKind } from "@/api/types-agent-runs";
import { SpanTree } from "./SpanTree";
import {
  CATEGORY_STYLES,
  categoryOfSpan,
  spanColorForSpan,
} from "./span-colors";
import {
  KNOWN_ENGINE_EVENT_KINDS,
  engineEventStyle,
} from "./engine-event-kinds";

/** The complete SpanKind union — kept in lockstep with types-agent-runs.ts. */
const ALL_SPAN_KINDS: SpanKind[] = [
  "agent.run",
  "agent.plan",
  "agent.decision",
  "decision.model",
  "decision.reasoning",
  "model.call",
  "model.reasoning",
  "tool.call",
  "tool.validate_input",
  "tool.validate_output",
  "approval.request",
  "approval.response",
  "sandbox.exec",
  "supervisor.review",
  "financial.eval",
  "artifact.write",
  "ipc.notification",
  "skill.invoke",
  "broker.call",
  "recovery.attempt",
  "state.transition",
  "engine.event",
];

function spanFixture(over: Partial<RunSpan> & Pick<RunSpan, "span_id" | "kind">): RunSpan {
  return {
    parent_span_id: null,
    name: over.kind,
    started_at: "2026-06-14T10:00:00.000Z",
    finished_at: "2026-06-14T10:00:01.000Z",
    status: "ok",
    attributes: {},
    ...over,
  };
}

const FALLBACK_LABEL = CATEGORY_STYLES.unknown.label;

describe("WS-8 render parity — every kind renders a row, nothing dropped", () => {
  test("every SpanKind renders a row with a non-fallback badge", () => {
    // `engine.event` is rendered via its engine-event family (covered below);
    // here we assert the ordinary span kinds get a confident, typed badge.
    const ordinary = ALL_SPAN_KINDS.filter((k) => k !== "engine.event");
    const spans = ordinary.map((kind, i) =>
      spanFixture({ span_id: `s_${i}`, kind, name: `${kind} row` }),
    );
    render(
      <SpanTree spans={spans} selectedSpanId={null} onSelect={() => {}} />,
    );
    for (let i = 0; i < ordinary.length; i++) {
      const kind = ordinary[i];
      const row = screen.getByTestId(`span-tree-row-s_${i}`);
      expect(row, `SpanKind "${kind}" produced no row`).toBeInTheDocument();
      const style = spanColorForSpan(spanFixture({ span_id: "x", kind }));
      // The row's badge must be the kind's typed label — never the EVENT
      // fallback (which would mean the kind fell through uncategorised).
      expect(
        within(row).getByText(style.label),
        `SpanKind "${kind}" rendered the fallback badge "${FALLBACK_LABEL}"`,
      ).toBeInTheDocument();
      expect(style.label).not.toBe(FALLBACK_LABEL);
      expect(categoryOfSpan(spanFixture({ span_id: "x", kind }))).not.toBe("unknown");
    }
  });

  test("every known EngineEvent.kind renders an engine.event row with its family badge", () => {
    const spans = KNOWN_ENGINE_EVENT_KINDS.map((kind, i) =>
      spanFixture({
        span_id: `ee_${i}`,
        kind: "engine.event",
        name: kind,
        attributes: { engine_event_kind: kind },
      }),
    );
    render(
      <SpanTree spans={spans} selectedSpanId={null} onSelect={() => {}} />,
    );
    for (let i = 0; i < KNOWN_ENGINE_EVENT_KINDS.length; i++) {
      const kind = KNOWN_ENGINE_EVENT_KINDS[i];
      const row = screen.getByTestId(`span-tree-row-ee_${i}`);
      expect(row, `engine kind "${kind}" produced no row`).toBeInTheDocument();
      const familyLabel = engineEventStyle(kind).label;
      // A KNOWN engine kind must render its FAMILY badge, never the unknown
      // EVENT fallback.
      expect(
        familyLabel,
        `engine kind "${kind}" fell through to the EVENT fallback`,
      ).not.toBe(FALLBACK_LABEL);
      expect(within(row).getByText(familyLabel)).toBeInTheDocument();
    }
  });

  test("an UNKNOWN span kind renders the typed fallback row (not dropped)", () => {
    const span = spanFixture({
      span_id: "unk_span",
      kind: "future.unforeseen.kind" as SpanKind,
      name: "future.unforeseen.kind",
    });
    render(
      <SpanTree spans={[span]} selectedSpanId={null} onSelect={() => {}} />,
    );
    const row = screen.getByTestId("span-tree-row-unk_span");
    // The row exists — the kind is NOT discarded.
    expect(row).toBeInTheDocument();
    // It carries the typed fallback badge AND the raw kind/name so the
    // operator can still see what fired.
    expect(within(row).getByText(FALLBACK_LABEL)).toBeInTheDocument();
    expect(within(row).getByText("future.unforeseen.kind")).toBeInTheDocument();
  });

  test("an UNKNOWN engine-event kind renders the typed fallback row (not dropped)", () => {
    const span = spanFixture({
      span_id: "unk_engine",
      kind: "engine.event",
      name: "brand_new_engine_signal",
      attributes: { engine_event_kind: "brand_new_engine_signal" },
    });
    render(
      <SpanTree spans={[span]} selectedSpanId={null} onSelect={() => {}} />,
    );
    const row = screen.getByTestId("span-tree-row-unk_engine");
    expect(row).toBeInTheDocument();
    // Unknown engine kind → neutral EVENT fallback badge, raw kind still shown.
    expect(within(row).getByText(FALLBACK_LABEL)).toBeInTheDocument();
    expect(within(row).getByText("brand_new_engine_signal")).toBeInTheDocument();
  });

  test("known engine families are visually distinct from each other (no flat blob)", () => {
    // Sanity: the families used by known kinds resolve to >1 distinct color,
    // so the engine-event band reads as categorised, not a single tint.
    const colors = new Set(
      KNOWN_ENGINE_EVENT_KINDS.map((k) => engineEventStyle(k).hex),
    );
    expect(colors.size).toBeGreaterThan(1);
  });
});
