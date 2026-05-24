// Signal metadata chip — the affordance under the run H1 for the run's
// contextual records (Strategy / Scenario / Agent). UPPERCASE tracked label +
// mono value + trailing chevron. README §3 / Task B4 step 5.
//
// Three tones map to the three contexts: `gold` (Strategy, brand-coded),
// `neutral` (Scenario), `info` (Agent). Rendered as a `<button>` so it carries
// the click affordance even when it routes via `onClick` (the parent wires a
// `navigate(...)`); the chevron is the visible cue.

export type MetaChipTone = "neutral" | "gold" | "info";

type ToneStyle = { color: string; bd: string; bg: string; lbl: string };

const TONES: Record<MetaChipTone, ToneStyle> = {
  neutral: {
    color: "var(--text)",
    bd: "var(--border)",
    bg: "var(--surface-elev)",
    lbl: "var(--text-3)",
  },
  gold: {
    color: "var(--gold)",
    bd: "var(--gold-soft)",
    bg: "var(--gold-bg)",
    lbl: "var(--gold-soft)",
  },
  info: {
    color: "var(--info)",
    bd: "rgba(95,168,255,0.40)",
    bg: "rgba(95,168,255,0.10)",
    lbl: "var(--info)",
  },
};

export function MetaChip({
  label,
  value,
  tone = "neutral",
  onClick,
  chevron = true,
  ariaLabel,
}: {
  label: string;
  value: string;
  tone?: MetaChipTone;
  onClick?: () => void;
  chevron?: boolean;
  ariaLabel?: string;
}) {
  const t = TONES[tone];
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={ariaLabel}
      className="inline-flex items-center gap-2 transition-colors"
      style={{
        height: 28,
        padding: "0 10px",
        background: t.bg,
        border: `1px solid ${t.bd}`,
        borderRadius: 4,
        cursor: onClick ? "pointer" : "default",
      }}
    >
      <span
        className="font-mono uppercase"
        style={{ fontSize: 10, letterSpacing: "0.16em", color: t.lbl, fontWeight: 600 }}
      >
        {label}
      </span>
      <span className="font-mono break-all" style={{ fontSize: 12, color: t.color, fontWeight: 500 }}>
        {value}
      </span>
      {chevron && (
        <svg
          width="9"
          height="9"
          viewBox="0 0 12 12"
          fill="none"
          aria-hidden
          style={{ opacity: 0.5, marginLeft: 1, flexShrink: 0 }}
        >
          <path
            d="M4.5 2.5L8 6l-3.5 3.5"
            stroke={t.color}
            strokeWidth="1.4"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      )}
    </button>
  );
}
