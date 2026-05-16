import { CANONICAL_AGENT_PROFILES } from "@/api/eval-review";

/// Inline pill-style picker. We render one button per canonical persona
/// (`fast-trader-agent`, `reasoning-agent`, `risk-agent`,
/// `research-agent`). The dashboard doesn't have a
/// `/api/agent-profiles` endpoint yet — when it lands, this list can
/// be replaced with a real fetch; until then the canonical seeds are
/// the source of truth (migration 016 + xvision-core::agent_profiles).
export function AgentPicker({
  selected,
  busy,
  onSelect,
}: {
  selected: string | null;
  busy: boolean;
  onSelect: (id: string) => void;
}) {
  return (
    <div className="flex flex-wrap gap-2">
      {CANONICAL_AGENT_PROFILES.map((p) => {
        const isSelected = p.id === selected;
        return (
          <button
            key={p.id}
            type="button"
            onClick={() => onSelect(p.id)}
            disabled={busy}
            aria-pressed={isSelected}
            title={p.blurb}
            className={[
              "px-3 py-1.5 rounded-sm text-[12px] border transition-colors",
              isSelected
                ? "bg-gold border-gold text-bg font-medium"
                : "border-border text-text-2 hover:border-gold/60 hover:text-text",
              busy ? "opacity-50 cursor-wait" : "",
            ].join(" ")}
          >
            {p.label}
          </button>
        );
      })}
    </div>
  );
}
