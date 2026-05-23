/**
 * StrategyRosterPills — horizontal pill rail listing every available
 * strategy. Each pill is a toggle:
 *   - ON  : tinted with the strategy's color (bg ~8%, border ~33%),
 *     full-color label, swatch dot.
 *   - OFF : muted (border-soft, text-3), greyed dot.
 * Click toggles inclusion. The `×` micro-button removes a strategy
 * directly (disabled when count = min selection).
 *
 * Stable order = the rotation order so the rail reads predictably as
 * strategies come and go.
 */
import type { ReactElement } from "react";

export interface RosterPillItem {
  id: string;
  label: string;
  color: string;
}

export interface StrategyRosterPillsProps {
  available: RosterPillItem[];
  selectedIds: string[];
  onToggle: (id: string) => void;
  onRemove: (id: string) => void;
  canRemove: (id: string) => boolean;
}

export function StrategyRosterPills({
  available,
  selectedIds,
  onToggle,
  onRemove,
  canRemove,
}: StrategyRosterPillsProps): ReactElement {
  return (
    <div
      className="flex flex-wrap items-center gap-2"
      role="group"
      aria-label="Strategy roster"
    >
      {available.map((it) => {
        const isSelected = selectedIds.includes(it.id);
        const removable = isSelected && canRemove(it.id);
        return (
          <span
            key={it.id}
            className="inline-flex items-center gap-2 rounded-full border pl-2 pr-1 py-1 text-[12px] transition-colors"
            style={
              isSelected
                ? {
                    backgroundColor: `${it.color}14`,
                    borderColor: `${it.color}55`,
                    color: "var(--text)",
                  }
                : {
                    backgroundColor: "transparent",
                    borderColor: "var(--border-soft)",
                    color: "var(--text-3)",
                  }
            }
          >
            <button
              type="button"
              className="inline-flex items-center gap-1.5"
              onClick={() => onToggle(it.id)}
              aria-pressed={isSelected}
              aria-label={isSelected ? `Disable ${it.label}` : `Enable ${it.label}`}
            >
              <span
                aria-hidden="true"
                className="inline-block w-2 h-2 rounded-full"
                style={{
                  backgroundColor: isSelected ? it.color : "var(--text-4)",
                  boxShadow: isSelected ? `0 0 0 3px ${it.color}1a` : "none",
                }}
              />
              <span>{it.label}</span>
            </button>
            {isSelected && (
              <button
                type="button"
                className="inline-flex items-center justify-center w-4 h-4 rounded-full text-text-3 hover:text-text disabled:opacity-30 disabled:cursor-not-allowed"
                onClick={() => onRemove(it.id)}
                disabled={!removable}
                aria-label={`Remove ${it.label}`}
                title={removable ? "Remove" : "At least two strategies required"}
              >
                ×
              </button>
            )}
          </span>
        );
      })}
    </div>
  );
}
