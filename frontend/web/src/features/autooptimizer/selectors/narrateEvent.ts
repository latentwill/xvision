import type { CycleProgressEvent, PersistedCycleEvent } from "../api";

export type NarrationTone = "kept" | "rejected" | "suspect" | "warn" | "neutral";
export type Narration = { sentence: string; tone: NarrationTone; hash: string | null };

const short = (h?: string | null) => (h ? h.slice(0, 8) : "");

const fmtDelta = (d: unknown): string | null => {
  if (typeof d !== "number") return null;
  const sign = d >= 0 ? "+" : "−";
  return `${sign}${Math.abs(d).toFixed(2)}`;
};

/**
 * Normalize a persisted cycle event row into a live `CycleProgressEvent`.
 *
 * Persisted rows store the full serialized event in `payload_json` (flattened,
 * `"type"`-tagged). Spread it back to top level; map the 3-way persisted kinds
 * (`mutation_gated_passed` / `mutation_gated_suspect` / `mutation_gated_dropped`)
 * to the serde discriminant; the row's `ts` wins so the feed has stable times.
 */
export function normalizePersisted(e: PersistedCycleEvent): CycleProgressEvent {
  let parsed: Record<string, unknown> = {};
  try {
    const p = JSON.parse(e.payload_json) as unknown;
    if (typeof p === "object" && p !== null && !Array.isArray(p)) {
      parsed = p as Record<string, unknown>;
    }
  } catch {
    parsed = {};
  }

  // The payload_json already has the "type" discriminant from serde; fall back
  // to mapping the 3-way kind names when the payload_json lacked it.
  const type =
    (parsed.type as string | undefined) ??
    e.kind.replace(/^mutation_gated_(passed|suspect|dropped)$/, "mutation_gated");

  return { ...parsed, type, cycle_id: e.cycle_id, ts: e.ts } as CycleProgressEvent;
}

/**
 * Convert one optimizer event (live SSE or normalized persisted) into a
 * human-readable narration sentence with a tone classification.
 *
 * Wire-shape ground truth (progress.rs): events are serde-tagged
 * `{"type":"mutation_gated", ...fields flattened at top level}` — there is NO
 * `payload` envelope. All fields are read directly from the event object.
 */
export function narrateEvent(e: CycleProgressEvent): Narration {
  // Support all three discriminant field names that may appear in practice.
  const kind = (e.type ?? (e as Record<string, unknown>).event_type ?? e.kind ?? "") as string;
  const x = e as Record<string, unknown>; // flattened wire fields
  const hash = (x.child_hash ?? x.bundle_hash ?? null) as string | null;

  switch (kind) {
    case "cycle_started":
      return {
        sentence: `Cycle ${(x.cycle_id as string | undefined) ?? "?"} started · ${x.parent_count ?? "?"} parents`,
        tone: "neutral",
        hash,
      };

    case "parent_selected":
      return {
        sentence: `Parent selected: ${short(x.parent_hash as string | undefined)}`,
        tone: "neutral",
        hash: (x.parent_hash as string | null | undefined) ?? null,
      };

    case "mutation_proposed": {
      const writer = (x.mutator_model as string | undefined) || "writer";
      return {
        sentence: `Writer ${writer} proposed an experiment → ${short(hash)}`,
        tone: "neutral",
        hash,
      };
    }

    case "no_candidate": {
      const reason = x.reason ? ` — ${x.reason as string}` : "";
      return {
        sentence: `No experiment produced for ${short(x.parent_hash as string | undefined)}${reason}`,
        tone: "warn",
        hash,
      };
    }

    case "mutation_gated": {
      const delta = fmtDelta(x.delta_day);
      const deltaStr = delta ? ` · ΔSharpe ${delta}` : "";
      if (x.outcome === "suspect") {
        return {
          sentence: `Gate flagged ${short(hash)}${deltaStr} — suspect`,
          tone: "suspect",
          hash,
        };
      }
      if (x.passed === true || x.outcome === "kept") {
        return {
          sentence: `Gate passed ${short(hash)}${deltaStr} — kept`,
          tone: "kept",
          hash,
        };
      }
      return {
        sentence: `Gate failed ${short(hash)}${deltaStr} — rejected`,
        tone: "rejected",
        hash,
      };
    }

    case "honesty_check_run": {
      const msg = (x.message as string | undefined);
      const msgStr = msg ? ` — ${msg}` : "";
      if (x.passed) {
        return { sentence: `Honesty check passed${msgStr}`, tone: "kept", hash };
      }
      return { sentence: `Honesty check failed${msgStr} — results suspect`, tone: "suspect", hash };
    }

    case "judge_finding": {
      const severity = (x.severity as string | undefined) ?? "info";
      const code = (x.code as string | undefined) ?? "finding";
      return {
        sentence: `Judge (${severity}): ${code} on ${short(hash)}`,
        tone: "warn",
        hash,
      };
    }

    case "cycle_finished":
      return {
        sentence: `Cycle finished — ${x.active_count ?? 0} kept · ${x.suspect_count ?? 0} suspect · ${x.rejected_count ?? 0} rejected`,
        tone: "neutral",
        hash,
      };

    case "phase_started": {
      const phase = (x.phase as string | undefined) ?? "?";
      const detail = x.detail ? ` — ${x.detail as string}` : "";
      return {
        sentence: `Phase ${phase} started${detail}`,
        tone: "neutral",
        hash,
      };
    }

    case "phase_finished": {
      const phase = (x.phase as string | undefined) ?? "?";
      return { sentence: `Phase ${phase} finished`, tone: "neutral", hash };
    }

    case "session_state_changed":
      return {
        sentence: `Run state changed to ${(x.state as string | undefined) ?? "?"}`,
        tone: "neutral",
        hash,
      };

    case "flywheel_compiled":
      return { sentence: "Findings compiled into prompt pattern", tone: "neutral", hash };

    default:
      return { sentence: kind || "event", tone: "neutral", hash };
  }
}
