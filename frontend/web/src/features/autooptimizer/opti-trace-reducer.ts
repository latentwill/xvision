// frontend/web/src/features/autooptimizer/opti-trace-reducer.ts
//
// WS-11a — OPTI trace scope reducer.
//
// Projects the EXISTING autooptimizer cycle SSE stream (`CycleProgressEvent`,
// served from `GET /api/autooptimizer/events` and consumed by
// `useCycleEventStream`) into trace-dock rows (`RunSpan[]`) under a dedicated
// `opti.*` kind taxonomy. This is scope/render wiring — it invents no new
// instrumentation and opens no second EventSource. The caller feeds the rows
// it already has from the shared SSE subscription.
//
// Row taxonomy (one `RunSpan.kind` per cycle phase):
//   opti.cycle      — the cycle root; opens on CycleStarted, closes on CycleFinished
//   opti.parent     — ParentSelected
//   opti.experiment — MutationProposed (one per candidate; nests under the cycle)
//   opti.gate       — MutationGated (nests under its experiment, matched by hash)
//   opti.eval-run   — WS-11b: the candidate's persisted eval run, nested under
//                     its experiment. A navigable drill-link node (links to the
//                     run's /agent-runs/:runId trace), NOT an inline span-tree.
//   opti.honesty    — HonestyCheckRun (per-cycle null-result canary)
//   opti.judge      — JudgeFinding (nests under its experiment, matched by hash)
//   opti.flywheel   — FlywheelCompiled (DSPy compile step)
//
// Operator-surface labels come from the existing `formatEventLabel` (terminology
// lock); each event's salient fields (gate outcome + day-Sharpe delta, judge
// severity/code, honesty pass/fail) ride in `span.attributes` for the inspector.
//
// Scope note: gate tone is encoded via `RunSpan.status` —
//   kept    → "ok"          (Active, positive)
//   suspect → "in_progress" (Suspect, warn — span-colors maps opti.gate suspect→warn)
//   rejected→ "error"       (Rejected, muted/danger)
// The suspect tone can't be read off `status` alone (ok/error/in_progress is the
// closed set), so consumers key off `attributes.outcome` for the three-way
// kept/suspect/rejected split; `span-colors` does the same for opti.gate.

import type { RunSpan, SpanStatus } from "@/api/types-agent-runs";
import { formatEventLabel, type CycleProgressEvent } from "./api";

/** Stable, deterministic span id for a cycle root. */
export function OPTI_CYCLE_ROOT_ID(cycleId: string): string {
  return `opti-cycle:${cycleId}`;
}

/** Normalized three-way gate outcome (developer-surface; labels apply the lock). */
export type OptiGateOutcome = "kept" | "suspect" | "rejected";

function eventType(e: CycleProgressEvent): string {
  return (e.event_type ?? e.type ?? e.kind ?? "") as string;
}

function readString(e: CycleProgressEvent, key: string): string | undefined {
  const v = (e as Record<string, unknown>)[key];
  return typeof v === "string" ? v : undefined;
}

function tsOf(e: CycleProgressEvent, fallbackIdx: number): string {
  return e.ts ?? new Date(fallbackIdx).toISOString();
}

/** Normalize the gate event's three-way outcome from its (possibly partial) fields. */
export function classifyGate(e: CycleProgressEvent): OptiGateOutcome {
  const outcome = readString(e, "outcome");
  if (outcome === "suspect" || outcome === "quarantined") return "suspect";
  if (outcome === "kept" || outcome === "passed") return "kept";
  if (outcome === "dropped" || outcome === "rejected") return "rejected";
  const passed = (e as Record<string, unknown>).passed;
  if (passed === true) return "kept";
  return "rejected";
}

function gateStatus(outcome: OptiGateOutcome): SpanStatus {
  if (outcome === "kept") return "ok";
  if (outcome === "rejected") return "error";
  return "in_progress"; // suspect — span-colors keys the warn tone off attributes.outcome
}

/** Salient fields per event kind, copied onto `span.attributes` for the inspector. */
function attributesFor(e: CycleProgressEvent): Record<string, unknown> {
  const attrs: Record<string, unknown> = {};
  const copyKeys = [
    "cycle_id",
    "parent_hash",
    "child_hash",
    "bundle_hash",
    "mutator_model",
    "delta_day",
    "outcome",
    "passed",
    "message",
    "severity",
    "code",
    "reason",
    "parent_count",
    "active_count",
    "suspect_count",
    "rejected_count",
    "eval_run_id",
  ];
  for (const k of copyKeys) {
    const v = (e as Record<string, unknown>)[k];
    if (v !== undefined) attrs[k] = v;
  }
  return attrs;
}

/**
 * Project a buffered `CycleProgressEvent[]` (oldest-first) into trace-dock rows.
 *
 * The cycle root is synthesized lazily: the first event that names a cycle
 * opens one (so a page reload landing mid-cycle still produces a well-formed
 * tree). Experiments index by `child_hash` so gate + judge rows nest under
 * their candidate; rows whose hash matches no experiment fall back under the
 * cycle root.
 */
export function projectOptiRows(events: CycleProgressEvent[]): RunSpan[] {
  if (events.length === 0) return [];

  // The cycle id for this buffer. Prefer an explicit cycle_started, else the
  // first event that carries a cycle_id.
  let cycleId: string | null = null;
  for (const e of events) {
    const cid = readString(e, "cycle_id");
    if (cid) {
      cycleId = cid;
      break;
    }
  }
  if (!cycleId) cycleId = "cycle";

  const rootId = OPTI_CYCLE_ROOT_ID(cycleId);
  const rows: RunSpan[] = [];

  // Synthesize the cycle root up front so every later row has a parent.
  const cycleStartedEvent = events.find((e) => eventType(e) === "cycle_started");
  const cycleFinishedEvent = events.find((e) => eventType(e) === "cycle_finished");
  const rootStart = cycleStartedEvent ? tsOf(cycleStartedEvent, 0) : tsOf(events[0], 0);
  // The root's label/name reflects the cycle's terminal state when finished,
  // else its start label. (formatEventLabel maps cycle_finished → operator copy.)
  const rootLabelEvent = cycleFinishedEvent ?? cycleStartedEvent;
  const root: RunSpan = {
    span_id: rootId,
    parent_span_id: null,
    name: rootLabelEvent ? formatEventLabel(rootLabelEvent) : "Optimizer cycle",
    kind: "opti.cycle",
    started_at: rootStart,
    finished_at: cycleFinishedEvent ? tsOf(cycleFinishedEvent, events.length) : null,
    status: cycleFinishedEvent ? "ok" : "in_progress",
    attributes: {
      cycle_id: cycleId,
      ...(cycleStartedEvent ? attributesFor(cycleStartedEvent) : {}),
      ...(cycleFinishedEvent ? attributesFor(cycleFinishedEvent) : {}),
    },
  };
  rows.push(root);

  // child_hash → experiment span id, so gate/judge rows nest under their candidate.
  const experimentByHash = new Map<string, string>();
  let seq = 0;

  for (const e of events) {
    const kind = eventType(e);
    const ts = tsOf(e, ++seq);
    const childHash = readString(e, "child_hash") ?? readString(e, "bundle_hash");

    switch (kind) {
      case "cycle_started":
      case "cycle_finished":
        // Folded into the synthesized root above.
        break;

      case "parent_selected": {
        rows.push({
          span_id: `opti-parent:${cycleId}:${seq}`,
          parent_span_id: rootId,
          name: formatEventLabel(e),
          kind: "opti.parent",
          started_at: ts,
          finished_at: ts,
          status: "ok",
          attributes: attributesFor(e),
        });
        break;
      }

      case "mutation_proposed": {
        const expId = `opti-exp:${cycleId}:${childHash ?? seq}`;
        if (childHash) experimentByHash.set(childHash, expId);
        rows.push({
          span_id: expId,
          parent_span_id: rootId,
          name: formatEventLabel(e),
          kind: "opti.experiment",
          started_at: ts,
          finished_at: null, // closed by its gate row, conceptually; left open for live read
          status: "in_progress",
          attributes: attributesFor(e),
        });
        break;
      }

      case "no_candidate": {
        // No experiment produced for the parent — surface as an experiment row
        // that immediately resolves to error so the operator sees the dead end.
        rows.push({
          span_id: `opti-nocand:${cycleId}:${seq}`,
          parent_span_id: rootId,
          name: formatEventLabel(e),
          kind: "opti.experiment",
          started_at: ts,
          finished_at: ts,
          status: "error",
          attributes: attributesFor(e),
        });
        break;
      }

      case "mutation_gated":
      case "mutation_gated_passed":
      case "mutation_gated_suspect":
      case "mutation_gated_dropped":
      case "gate_evaluated": {
        const outcome = classifyGate(e);
        const parentId =
          (childHash && experimentByHash.get(childHash)) || rootId;
        const evalRunId = readString(e, "eval_run_id");
        // Resolve the parent experiment now it's gated.
        if (childHash && experimentByHash.has(childHash)) {
          const expId = experimentByHash.get(childHash)!;
          const exp = rows.find((r) => r.span_id === expId);
          if (exp) {
            if (exp.finished_at == null) {
              exp.finished_at = ts;
              exp.status = gateStatus(outcome);
            }
            // WS-11b: surface the candidate's eval run id on the experiment so
            // the inspector/strip can show it alongside the experiment row.
            if (evalRunId) exp.attributes.eval_run_id = evalRunId;
          }
        }
        rows.push({
          span_id: `opti-gate:${cycleId}:${childHash ?? seq}`,
          parent_span_id: parentId,
          name: formatEventLabel(e),
          kind: "opti.gate",
          started_at: ts,
          finished_at: ts,
          status: gateStatus(outcome),
          attributes: { ...attributesFor(e), outcome },
        });
        // WS-11b: nest a navigable eval-run node under the experiment (the gate
        // resolves to either the candidate's experiment row or the cycle root
        // when the proposal wasn't buffered). Carries `eval_run_id` so the
        // SpanInspector can drill to the run's `/agent-runs/:runId` trace. This
        // is a drill-link node only — the eval run's full span tree is NOT
        // inlined here (a future step). Skipped when the gate carries no run id
        // (regime path / test-stub runner) so no dangling node is created.
        if (evalRunId) {
          rows.push({
            span_id: `opti-evalrun:${cycleId}:${childHash ?? seq}`,
            parent_span_id: parentId,
            name: "Eval run",
            kind: "opti.eval-run",
            started_at: ts,
            finished_at: ts,
            status: gateStatus(outcome),
            attributes: { eval_run_id: evalRunId, child_hash: childHash ?? "" },
          });
        }
        break;
      }

      case "honesty_check_run": {
        const passed = (e as Record<string, unknown>).passed === true;
        rows.push({
          span_id: `opti-honesty:${cycleId}:${seq}`,
          parent_span_id: rootId,
          name: formatEventLabel(e),
          kind: "opti.honesty",
          started_at: ts,
          finished_at: ts,
          status: passed ? "ok" : "error",
          attributes: attributesFor(e),
        });
        break;
      }

      case "judge_finding": {
        const severity = readString(e, "severity") ?? "info";
        const parentId =
          (childHash && experimentByHash.get(childHash)) || rootId;
        rows.push({
          span_id: `opti-judge:${cycleId}:${childHash ?? seq}:${seq}`,
          parent_span_id: parentId,
          name: formatEventLabel(e),
          kind: "opti.judge",
          started_at: ts,
          finished_at: ts,
          // risk findings read as error, warn as warn (encoded in-progress so
          // span-colors can tint it warn), info as ok.
          status: severity === "risk" ? "error" : severity === "warn" ? "in_progress" : "ok",
          attributes: attributesFor(e),
        });
        break;
      }

      case "flywheel_compiled": {
        rows.push({
          span_id: `opti-flywheel:${cycleId}:${seq}`,
          parent_span_id: rootId,
          name: formatEventLabel(e),
          kind: "opti.flywheel",
          started_at: ts,
          finished_at: ts,
          status: "ok",
          attributes: attributesFor(e),
        });
        break;
      }

      default:
        // Unmapped informational events (diversity_scored, phase_*, etc.) are
        // intentionally not projected as rows in WS-11a — the dock surfaces the
        // named cycle phases. They remain visible in the existing NarratedFeed.
        break;
    }
  }

  return rows;
}
