// frontend/web/src/components/eval-detail/SignalsUsedChips.tsx
//
// Full-width inline chip strip showing the DISTINCT signal tools used in a
// cycle/run. Renders nothing when `signals_used` is absent or empty (the
// backend does not yet surface per-run tool usage in `RunDetail` — this
// component is UI-ready and gates on data presence).
//
// The 6 known signal tool names and their friendly labels are defined here;
// any unrecognised name falls back to the raw name.
//
// Layout rule: full-width `flex flex-wrap` — no fourth column, no popup.
// See CLAUDE.md "Frontend layout rule: no right-side boxes when the chat
// rail is visible".

const SIGNAL_LABELS: Record<string, string> = {
  nansen_smart_money_flow: "Nansen: Smart-money flow",
  nansen_token_screener: "Nansen: Token screener",
  nansen_flow_intel: "Nansen: Flow intel",
  elfa_smart_mentions: "Elfa: Smart mentions",
  elfa_trending_tokens: "Elfa: Trending tokens",
  elfa_trending_narratives: "Elfa: Trending narratives",
};

const KNOWN_SIGNAL_TOOLS = new Set(Object.keys(SIGNAL_LABELS));

function isSignalTool(name: string): boolean {
  return KNOWN_SIGNAL_TOOLS.has(name);
}

/** A single signal-tool chip. */
function SignalChip({ name }: { name: string }) {
  const label = SIGNAL_LABELS[name] ?? name;
  const isNansen = name.startsWith("nansen_");
  return (
    <span
      data-testid="signal-chip"
      data-signal={name}
      className="inline-flex items-center gap-1.5 font-mono text-[11px] leading-none"
      style={{
        height: 26,
        padding: "0 9px",
        borderRadius: 4,
        border: `1px solid ${isNansen ? "rgba(155,110,255,0.35)" : "rgba(95,168,255,0.35)"}`,
        background: isNansen ? "rgba(155,110,255,0.08)" : "rgba(95,168,255,0.08)",
        color: isNansen ? "var(--info, #7b6fe8)" : "var(--info, #5fa8ff)",
        whiteSpace: "nowrap",
      }}
    >
      {label}
    </span>
  );
}

/**
 * Full-width inline row of signal-tool chips for a cycle/run detail page.
 *
 * Props:
 *   `signals_used` — optional list of tool names. When undefined or empty,
 *   renders nothing (no-op until the backend populates `RunDetail.signals_used`).
 */
export function SignalsUsedChips({
  signals_used,
}: {
  signals_used?: string[] | null;
}) {
  if (!signals_used || signals_used.length === 0) return null;

  const distinct = [...new Set(signals_used.filter(isSignalTool))];
  if (distinct.length === 0) return null;

  return (
    <div
      data-testid="signals-used-strip"
      className="flex flex-wrap gap-2 items-center"
      aria-label="Signal tools used in this run"
    >
      <span
        className="font-mono uppercase text-[9px] tracking-[0.16em] text-text-3"
        aria-hidden
      >
        Signals
      </span>
      {distinct.map((name) => (
        <SignalChip key={name} name={name} />
      ))}
    </div>
  );
}
