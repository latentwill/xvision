import type { RouteContextField } from "@/api/strategies";

const CONTEXT_OPTIONS: Array<{
  value: RouteContextField;
  label: string;
  hint: string;
}> = [
  {
    value: "market_snapshot",
    label: "Market snapshot",
    hint: "Current candle, quotes, and derived market facts.",
  },
  {
    value: "tool_state",
    label: "Tool state",
    hint: "Allowed non-secret tool outputs available to this route.",
  },
  {
    value: "available_targets",
    label: "Available targets",
    hint: "The branch targets the router may choose from.",
  },
  {
    value: "regime_summary",
    label: "Regime summary",
    hint: "Compact upstream regime classification.",
  },
];

export const ROUTE_CONTEXT_OPTIONS = CONTEXT_OPTIONS;

export function routeContextLabel(value: RouteContextField): string {
  return (
    CONTEXT_OPTIONS.find((option) => option.value === value)?.label ??
    value.replace(/_/g, " ")
  ).toLowerCase();
}

export function RouteContextFields({
  selected,
  onChange,
  disabled = false,
}: {
  selected: RouteContextField[];
  onChange: (next: RouteContextField[]) => void;
  disabled?: boolean;
}) {
  function toggle(value: RouteContextField) {
    if (selected.includes(value)) {
      onChange(selected.filter((field) => field !== value));
    } else {
      onChange([...selected, value]);
    }
  }

  return (
    <div className="space-y-2">
      <div className="text-[12px] uppercase tracking-wide text-text-3">
        Router context
      </div>
      <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
        {CONTEXT_OPTIONS.map((option) => {
          const checked = selected.includes(option.value);
          return (
            <label
              key={option.value}
              className={[
                "flex items-start gap-2 rounded border border-border-soft bg-surface-elev px-3 py-2 text-[12.5px] transition-colors",
                checked ? "border-gold/45 bg-gold/10" : "",
                disabled ? "cursor-not-allowed opacity-60" : "cursor-pointer hover:border-text-3",
              ].join(" ")}
            >
              <input
                type="checkbox"
                checked={checked}
                disabled={disabled}
                onChange={() => toggle(option.value)}
                className="mt-0.5 accent-gold"
              />
              <span className="min-w-0">
                <span className="block text-text">{option.label}</span>
                <span className="block text-[11px] leading-snug text-text-3">
                  {option.hint}
                </span>
              </span>
            </label>
          );
        })}
      </div>
      <p className="m-0 text-[12px] leading-snug text-text-3">
        Hidden from router: credentials, broker secrets, unselected tools,
        hidden memory, or fields not checked above.
      </p>
    </div>
  );
}
