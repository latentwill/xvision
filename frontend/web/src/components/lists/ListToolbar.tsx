import { forwardRef, useEffect, useRef, useState } from "react";

import { Icon, type IconName } from "@/components/primitives/Icon";
import { SignalSelectMenu } from "@/components/primitives/SignalMenu";

import { ListActiveChips } from "./ListActiveChips";
import {
  isFilterActive,
  type ActiveFilter,
  type ColumnState,
  type ListSearchState,
  type ListSortState,
} from "./useListState";
import type { ListColumn } from "./ListCard";

export type ListToolbarProps = {
  search?: ListSearchState;
  filters?: ActiveFilter[];
  sort?: ListSortState;
  actions?: React.ReactNode;
  density?: "full" | "compact";
  showSearch?: boolean;
  showSort?: boolean;
  showActiveChips?: boolean;
  clearAll?: () => void;
  columnState?: ColumnState;
  columns?: ListColumn[];
  autoHiddenKeys?: Set<string>;
};

export const ListToolbar = forwardRef<HTMLInputElement, ListToolbarProps>(
  function ListToolbar(props, searchRef) {
    const {
      search,
      filters = [],
      sort,
      actions,
      density = "full",
      showSearch = true,
      showSort = true,
      showActiveChips = true,
      clearAll,
      columnState,
      columns = [],
      autoHiddenKeys = new Set(),
    } = props;
    const compact = density === "compact";

    // Count hidden non-essential columns for the badge.
    const hiddenCount = columnState
      ? columns.filter(
          (c) =>
            !c.essential &&
            (!columnState.visibleKeys.has(c.key) || autoHiddenKeys.has(c.key)),
        ).length
      : 0;

    return (
      <div className="flex flex-col gap-2.5">
        <div
          className={`flex items-center flex-wrap ${compact ? "gap-1.5" : "gap-2"}`}
        >
          {showSearch && search && (
            <ListSearch
              ref={searchRef}
              value={search.value}
              onChange={search.setValue}
              placeholder={search.placeholder}
              compact={compact}
            />
          )}
          {filters.length > 0 && (
            <div className="flex items-center flex-wrap gap-1.5">
              {filters.map((f) => (
                <SignalSelectMenu
                  key={f.def.id}
                  label={f.def.label}
                  value={f.value}
                  options={f.def.options}
                  icon={f.def.icon as IconName | undefined}
                  active={isFilterActive(f)}
                  compact={compact}
                  onChange={f.setValue}
                />
              ))}
            </div>
          )}
          {showSort && sort && (
            <SignalSelectMenu
              label="Sort"
              icon="sliders"
              value={sort.value || sort.options[0]?.value || ""}
              options={sort.options}
              active={
                !!sort.options[0] && sort.value !== sort.options[0].value
              }
              compact={compact}
              minWidth={compact ? 120 : 180}
              onChange={sort.setValue}
            />
          )}
          {columnState && columns.length > 0 && (
            <ColumnPickerButton
              columns={columns}
              columnState={columnState}
              autoHiddenKeys={autoHiddenKeys}
              hiddenCount={hiddenCount}
              compact={compact}
            />
          )}
          {actions && (
            <div className="ml-auto flex items-center gap-2">{actions}</div>
          )}
        </div>

        {showActiveChips && !compact && (
          <ListActiveChips
            search={search}
            filters={filters}
            clearAll={clearAll}
          />
        )}
      </div>
    );
  },
);

function ColumnPickerButton({
  columns,
  columnState,
  autoHiddenKeys,
  hiddenCount,
  compact,
}: {
  columns: ListColumn[];
  columnState: ColumnState;
  autoHiddenKeys: Set<string>;
  hiddenCount: number;
  compact: boolean;
}) {
  const [open, setOpen] = useState(false);
  const panelRef = useRef<HTMLDivElement>(null);
  const btnRef = useRef<HTMLButtonElement>(null);

  // Close on outside click.
  useEffect(() => {
    if (!open) return;
    function onDown(e: MouseEvent) {
      if (
        !panelRef.current?.contains(e.target as Node) &&
        !btnRef.current?.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  // Columns available for the picker (all non-essential columns).
  const pickableColumns = columns.filter((c) => !c.essential);

  return (
    <div className="relative">
      <button
        ref={btnRef}
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className={[
          "inline-flex items-center gap-1.5 border rounded-sm cursor-pointer font-sans transition-colors",
          compact ? "h-8 px-2 text-[12px]" : "h-8 px-2.5 text-[13px]",
          open
            ? "border-gold-soft bg-gold-bg text-text"
            : "border-border bg-surface-elev text-text-2 hover:border-text-3 hover:text-text",
        ].join(" ")}
      >
        <Icon name="list" size={12} className="shrink-0" />
        Columns
        {hiddenCount > 0 && (
          <span className="inline-flex items-center justify-center h-4 min-w-[16px] px-1 rounded-sm bg-gold-bg border border-gold-soft text-gold font-mono text-[9px]">
            {hiddenCount}
          </span>
        )}
      </button>

      {open && (
        <div
          ref={panelRef}
          className="absolute left-0 top-full mt-1 z-30 w-56 rounded-md border border-border bg-surface-panel shadow-[0_8px_24px_rgba(0,0,0,.5)] py-2"
        >
          <div className="px-3 py-1 text-[10px] font-mono uppercase tracking-wider text-text-3">
            Columns
          </div>
          <div className="max-h-64 overflow-y-auto">
            {pickableColumns.map((c) => {
              const checked = columnState.visibleKeys.has(c.key);
              const isAuto = autoHiddenKeys.has(c.key);
              return (
                <label
                  key={c.key}
                  className="flex items-center gap-2.5 px-3 py-1.5 cursor-pointer hover:bg-surface-elev transition-colors"
                >
                  <input
                    type="checkbox"
                    checked={checked}
                    onChange={() => columnState.toggle(c.key)}
                    className="accent-gold w-3.5 h-3.5 shrink-0"
                  />
                  <span className="flex-1 text-[12.5px] text-text-2">{c.label}</span>
                  {isAuto && (
                    <span className="text-[10px] font-mono text-text-4 italic">auto</span>
                  )}
                </label>
              );
            })}
          </div>
          <div className="border-t border-border mt-1 pt-1 px-3">
            <button
              type="button"
              onClick={() => {
                columnState.reset();
                setOpen(false);
              }}
              className="w-full text-left text-[11px] text-text-3 hover:text-accent py-1 transition-colors"
            >
              Reset to defaults
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

type ListSearchProps = {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  compact?: boolean;
};

const ListSearch = forwardRef<HTMLInputElement, ListSearchProps>(
  function ListSearch({ value, onChange, placeholder = "Search…", compact }, ref) {
    const [open, setOpen] = useState(!compact);

    useEffect(() => {
      if (!compact) setOpen(true);
    }, [compact]);

    if (compact && !open) {
      return (
        <button
          type="button"
          onClick={() => setOpen(true)}
          aria-label="Search"
          title="Search"
          className="h-8 w-8 inline-flex items-center justify-center bg-transparent border border-border rounded-sm cursor-pointer transition-colors hover:border-text-3"
        >
          <Icon name="search" size={13} className="text-text-2" />
        </button>
      );
    }

    return (
      <div
        className="flex items-center gap-2 px-2.5 h-8 bg-surface-elev border border-border rounded-sm transition-colors focus-within:border-gold-soft flex-1 min-w-0"
        style={{ maxWidth: compact ? 200 : 280 }}
      >
        <Icon name="search" size={13} className="text-text-3" />
        <input
          ref={ref}
          autoFocus={compact}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder}
          spellCheck={false}
          className="flex-1 min-w-0 bg-transparent border-none outline-none text-text font-sans text-[13px] p-0 placeholder:text-text-3"
        />
        {value && (
          <button
            type="button"
            onClick={() => {
              onChange("");
              if (compact) setOpen(false);
            }}
            aria-label="Clear search"
            className="border-none bg-none cursor-pointer text-text-3 text-base leading-none px-0.5 hover:text-text"
          >
            ×
          </button>
        )}
        {!compact && (
          <span className="inline-flex items-center justify-center w-[18px] h-[18px] border border-border-strong rounded-[3px] font-mono text-[10px] text-text-3">
            /
          </span>
        )}
      </div>
    );
  },
);
