/**
 * AssetPicker — inline searchable, category-grouped asset picker.
 *
 * This is an inline combobox widget: the results list uses `absolute`
 * positioning relative to the picker's own container (z-10) to stay
 * compact when placed in a toolbar row. It does NOT steal focus from
 * the rest of the page, open a modal/sheet, or paint over the primary
 * surface — consistent with the dashboard no-popups rule (CLAUDE.md
 * adopted 2026-05-17).
 */
import * as React from "react";
import type { AssetInfo } from "@/api/assets";

interface Props {
  assets: AssetInfo[];
  value: string;
  onChange: (symbol: string) => void;
  /** If true, orderly-only assets are shown with a "no backtest data" badge. */
  showOrderlyOnlyBadge?: boolean;
  placeholder?: string;
  className?: string;
}

export function AssetPicker({
  assets,
  value,
  onChange,
  showOrderlyOnlyBadge,
  placeholder = "Search assets…",
  className,
}: Props) {
  const [query, setQuery] = React.useState("");
  const [open, setOpen] = React.useState(false);
  const containerRef = React.useRef<HTMLDivElement>(null);
  const inputRef = React.useRef<HTMLInputElement>(null);

  const filtered = React.useMemo(() => {
    const q = query.trim().toUpperCase();
    if (!q) return assets;
    return assets.filter(
      (a) =>
        a.symbol.toUpperCase().includes(q) ||
        a.category.toUpperCase().includes(q),
    );
  }, [assets, query]);

  // Group by category
  const grouped = React.useMemo(() => {
    const map = new Map<string, AssetInfo[]>();
    for (const a of filtered) {
      const list = map.get(a.category) ?? [];
      list.push(a);
      map.set(a.category, list);
    }
    return map;
  }, [filtered]);

  // Close when clicking outside
  React.useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (
        containerRef.current &&
        !containerRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
        setQuery("");
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  function openPicker() {
    setOpen(true);
    // Defer focus so the input is mounted/visible first
    window.requestAnimationFrame(() => inputRef.current?.focus());
  }

  const selectedAsset = assets.find((a) => a.symbol === value);

  return (
    <div ref={containerRef} className={`relative ${className ?? ""}`}>
      {/* Trigger row */}
      <div
        role="combobox"
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-label="Asset picker"
        className="flex items-center gap-2 h-9 px-3 rounded-md border border-border bg-background cursor-text text-sm"
        onClick={openPicker}
      >
        {open ? (
          <input
            ref={inputRef}
            className="flex-1 bg-transparent outline-none placeholder:text-muted-foreground text-[12px]"
            placeholder={placeholder}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onFocus={() => setOpen(true)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                setOpen(false);
                setQuery("");
              }
            }}
          />
        ) : (
          <>
            {selectedAsset ? (
              <>
                <span className="font-mono text-[12px] font-medium text-text">
                  {selectedAsset.symbol}
                </span>
                <span className="text-muted-foreground text-[11px] text-text-3">
                  {selectedAsset.category}
                </span>
                {showOrderlyOnlyBadge &&
                  selectedAsset.data === "orderly-only" && (
                    <span className="ml-auto text-[11px] text-amber-600 dark:text-amber-400 bg-amber-50 dark:bg-amber-950/40 border border-amber-200 dark:border-amber-500/30 rounded px-1.5">
                      no backtest data
                    </span>
                  )}
              </>
            ) : (
              <span className="text-text-3 text-[12px]">{placeholder}</span>
            )}
          </>
        )}
      </div>

      {/* Results list — z-10 overlay within this container only */}
      {open && (
        <div
          role="listbox"
          className="absolute z-10 mt-1 w-full max-h-64 overflow-y-auto rounded-md border border-border bg-background shadow-md"
        >
          {grouped.size === 0 ? (
            <div className="px-3 py-2 text-[12px] text-text-3">
              No assets found
            </div>
          ) : (
            Array.from(grouped.entries()).map(([cat, items]) => (
              <div key={cat}>
                <div className="px-3 py-1 text-[11px] font-semibold uppercase tracking-wide text-text-3 bg-surface-elev">
                  {cat}
                </div>
                {items.map((a) => (
                  <button
                    key={a.symbol}
                    type="button"
                    role="option"
                    aria-selected={a.symbol === value}
                    className={`w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-left hover:bg-surface-elev ${
                      a.symbol === value ? "bg-surface-elev font-medium" : ""
                    }`}
                    onClick={() => {
                      onChange(a.symbol);
                      setOpen(false);
                      setQuery("");
                    }}
                  >
                    <span className="flex-1 font-mono text-text">
                      {a.symbol}
                    </span>
                    {showOrderlyOnlyBadge && a.data === "orderly-only" && (
                      <span className="text-[11px] text-amber-600 dark:text-amber-400 bg-amber-50 dark:bg-amber-950/40 border border-amber-200 dark:border-amber-500/30 rounded px-1.5">
                        no backtest data
                      </span>
                    )}
                  </button>
                ))}
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}
