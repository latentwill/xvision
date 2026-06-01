import { forwardRef, useEffect, useId, useState } from "react";
import type { CSSProperties } from "react";

import { Icon, type IconName } from "@/components/primitives/Icon";

import { ListActiveChips } from "./ListActiveChips";
import {
  isFilterActive,
  type ActiveFilter,
  type ListSearchState,
  type ListSortState,
} from "./useListState";

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
    } = props;
    const compact = density === "compact";

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
                <ListSelect
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
            <ListSelect
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
        className="flex items-center gap-2 px-2.5 h-8 bg-surface-elev border border-border rounded-sm transition-colors focus-within:border-gold-soft"
        style={{ width: compact ? 200 : 280 }}
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

type ListSelectProps = {
  label: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (v: string) => void;
  icon?: IconName;
  active?: boolean;
  compact?: boolean;
  minWidth?: number;
};

function ListSelect({
  label,
  value,
  options,
  onChange,
  icon,
  active = false,
  compact = false,
  minWidth,
}: ListSelectProps) {
  const id = useId();
  const selected = options.find((o) => o.value === value) ?? options[0];

  const labelStyle: CSSProperties | undefined = minWidth
    ? { minWidth }
    : undefined;

  return (
    <label
      htmlFor={id}
      data-active={active || undefined}
      style={labelStyle}
      className="relative inline-flex items-center gap-1.5 h-8 px-2.5 bg-surface-elev border border-border rounded-sm text-text-2 text-[12.5px] cursor-pointer transition-colors whitespace-nowrap hover:border-text-3 data-[active]:border-gold/45 data-[active]:bg-gold/10"
    >
      {icon && (
        <Icon
          name={icon}
          size={12}
          className={active ? "text-gold" : "text-text-3"}
        />
      )}
      {!compact && (
        <span className="text-text-3 text-[11.5px] tracking-wide">{label}</span>
      )}
      <span
        className={`font-medium text-[12.5px] ${active ? "text-gold" : "text-text"}`}
      >
        {selected?.label}
      </span>
      <Icon name="chevR" size={11} className="text-text-3" />
      <select
        id={id}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="absolute inset-0 opacity-0 cursor-pointer font-sans"
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}
