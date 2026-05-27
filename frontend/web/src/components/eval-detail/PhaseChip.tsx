// Signal phase chip — sits in the Decisions table PHASE column. Distinguishes a
// step where the trader engaged a decision (ENGAGED, filled green dot) from a
// synthesized no-op step (NO-OP, hollow ring) — e.g. a hold/flat row inherited
// from an early-stop policy or `noop_skip` marker. The internal `Phase`
// discriminant is still `"filtered"` to keep the wire shape and existing tests
// stable; only the user-visible label was renamed so it doesn't collide with
// the engine-level filter (FilterGated) suppression which is a separate
// concept surfaced by `FilterSummaryPanel` / `FilterEventTimeline`.
//
// Design intent (README §6): NO-OP must read *quieter* than ENGAGED but NOT as
// an error — no red, no amber. A synthesized step is a normal outcome.
// Achieved via hollow ring + lighter text + transparent background.

export type Phase = "engaged" | "filtered";

export function PhaseChip({ phase }: { phase: Phase }) {
  const filtered = phase === "filtered";
  return (
    <span
      className="inline-flex items-center gap-1.5 font-mono uppercase"
      style={{
        color: filtered ? "var(--text-3)" : "var(--text)",
        background: filtered ? "transparent" : "var(--surface-elev)",
        border: "1px solid var(--border-strong)",
        padding: "3px 8px",
        borderRadius: 3,
        fontSize: 10,
        fontWeight: filtered ? 500 : 600,
        letterSpacing: "0.12em",
        lineHeight: 1,
      }}
    >
      <span
        aria-hidden
        style={{
          width: 5,
          height: 5,
          borderRadius: "50%",
          background: filtered ? "transparent" : "var(--gold)",
          border: filtered ? "1px solid var(--text-3)" : "none",
        }}
      />
      {filtered ? "NO-OP" : "ENGAGED"}
    </span>
  );
}
