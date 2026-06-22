// Signal floating menu system.
//
// Provides three variants: SignalActionMenu (⋯ overflow), SignalSelectMenu
// (single-select with checkmark), SignalCheckboxMenu (multi-select), and
// useSignalMenu (hook for custom compositions).
//
// All menus render in a portal at document.body, positioned via fixed coords
// relative to their trigger's getBoundingClientRect(). They close on
// click-outside and Escape.

import { createPortal } from "react-dom";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import type { CSSProperties, KeyboardEvent as ReactKeyboardEvent, ReactNode, RefObject } from "react";
import { Icon, type IconName } from "./Icon";

// ─── Floating position hook ───────────────────────────────────────────────────

function calcPos(
  trigger: DOMRect,
  menuW: number,
  menuH: number,
  align: "left" | "right",
): CSSProperties {
  const vw = window.innerWidth;
  const vh = window.innerHeight;

  let left: number;
  if (align === "right") {
    left = trigger.right - menuW;
  } else {
    left = trigger.left;
  }

  // Keep within viewport horizontally
  if (left + menuW > vw - 8) left = vw - menuW - 8;
  if (left < 8) left = 8;

  let top = trigger.bottom + 4;
  // Flip up if not enough room below
  if (top + menuH > vh - 8) {
    top = trigger.top - menuH - 4;
  }
  if (top < 8) top = 8;

  return { position: "fixed", left, top, zIndex: 9999 };
}

export function useSignalMenu(align: "left" | "right" = "left") {
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<CSSProperties>({});

  const openMenu = useCallback(() => {
    if (!triggerRef.current) return;
    const rect = triggerRef.current.getBoundingClientRect();
    // estimate initial size; will refine after render
    setPos(calcPos(rect, 240, 300, align));
    setOpen(true);
  }, [align]);

  // Refine position once menu is rendered and has a real size
  useEffect(() => {
    if (!open || !menuRef.current || !triggerRef.current) return;
    const menuRect = menuRef.current.getBoundingClientRect();
    const triggerRect = triggerRef.current.getBoundingClientRect();
    setPos(calcPos(triggerRect, menuRect.width, menuRect.height, align));
  }, [open, align]);

  // Close on outside click or Escape
  useEffect(() => {
    if (!open) return;
    function onPointerDown(e: MouseEvent) {
      if (
        !menuRef.current?.contains(e.target as Node) &&
        !triggerRef.current?.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", onPointerDown, true);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onPointerDown, true);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const toggle = useCallback(() => {
    if (open) setOpen(false);
    else openMenu();
  }, [open, openMenu]);

  return { open, setOpen, openMenu, toggle, triggerRef, menuRef, pos };
}

// ─── Menu shell ───────────────────────────────────────────────────────────────

interface MenuShellProps {
  open: boolean;
  menuRef: RefObject<HTMLDivElement>;
  pos: CSSProperties;
  children: ReactNode;
  minWidth?: number;
  className?: string;
  role?: "menu" | "listbox";
  id?: string;
}

function MenuShell({ open, menuRef, pos, children, minWidth = 220, className, role = "menu", id }: MenuShellProps) {
  if (!open) return null;
  return createPortal(
    <div
      ref={menuRef}
      role={role}
      id={id}
      style={{ ...pos, minWidth }}
      className={[
        "rounded-[6px] border border-border bg-surface-card",
        "shadow-[0_8px_24px_rgba(0,0,0,0.6)] py-1 outline-none",
        className ?? "",
      ].join(" ")}
    >
      {children}
    </div>,
    document.body,
  );
}

// ─── Shared menu item atoms ────────────────────────────────────────────────────

interface ActionItemProps {
  icon?: IconName;
  label: string;
  shortcut?: string;
  tone?: "default" | "danger";
  disabled?: boolean;
  onClick: () => void;
}

export function SignalMenuItem({ icon, label, shortcut, tone = "default", disabled, onClick }: ActionItemProps) {
  return (
    <button
      type="button"
      role="menuitem"
      disabled={disabled}
      onClick={onClick}
      className={[
        "flex w-full items-center gap-2.5 px-3 h-[34px] text-left",
        "text-[13px] transition-colors",
        "disabled:cursor-not-allowed disabled:opacity-40",
        tone === "danger"
          ? "text-danger hover:bg-[rgba(255,77,77,0.08)]"
          : "text-text hover:bg-surface-hover",
      ].join(" ")}
    >
      {icon ? (
        <Icon name={icon} size={14} className={tone === "danger" ? "text-danger" : "text-text-3"} />
      ) : (
        <span className="w-[14px]" />
      )}
      <span className="flex-1">{label}</span>
      {shortcut && (
        <span className="font-mono text-[11px] text-text-4">{shortcut}</span>
      )}
    </button>
  );
}

export function SignalMenuSeparator() {
  return <div className="my-1 h-px bg-border-soft" />;
}

export function SignalMenuLabel({ children }: { children: ReactNode }) {
  return (
    <div className="px-3 pt-2 pb-1 font-mono text-[10px] font-medium tracking-[0.16em] uppercase text-text-3">
      {children}
    </div>
  );
}

// ─── SignalActionMenu ─────────────────────────────────────────────────────────

export interface ActionMenuItem {
  icon?: IconName;
  label: string;
  shortcut?: string;
  tone?: "default" | "danger";
  disabled?: boolean;
  onClick: () => void;
}

export interface ActionMenuGroup {
  items: ActionMenuItem[];
}

export interface SignalActionMenuProps {
  groups: ActionMenuGroup[];
  triggerLabel?: ReactNode;
  triggerAriaLabel?: string;
  align?: "left" | "right";
  triggerClassName?: string;
}

export function SignalActionMenu({
  groups,
  triggerLabel = "⋯",
  triggerAriaLabel,
  align = "right",
  triggerClassName,
}: SignalActionMenuProps) {
  const { open, setOpen, toggle, triggerRef, menuRef, pos } = useSignalMenu(align);

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={triggerAriaLabel}
        onClick={toggle}
        className={[
          triggerClassName ??
            "inline-flex h-7 w-7 items-center justify-center rounded text-text-3 transition-colors hover:bg-surface-hover hover:text-text",
          "focus:outline-none focus:border-gold/45 focus-visible:ring-1 focus-visible:ring-gold/45",
        ].join(" ")}
      >
        {triggerLabel}
      </button>
      <MenuShell open={open} menuRef={menuRef as RefObject<HTMLDivElement>} pos={pos} minWidth={220}>
        {groups.map((group, gi) => (
          <div key={gi}>
            {gi > 0 && <SignalMenuSeparator />}
            {group.items.map((item, ii) => (
              <SignalMenuItem
                key={ii}
                icon={item.icon}
                label={item.label}
                shortcut={item.shortcut}
                tone={item.tone}
                disabled={item.disabled}
                onClick={() => {
                  item.onClick();
                  setOpen(false);
                }}
              />
            ))}
          </div>
        ))}
      </MenuShell>
    </>
  );
}

// ─── SignalSelectMenu ─────────────────────────────────────────────────────────
// Single-select dropdown with checkmark on chosen option.

export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

export interface SignalSelectMenuProps {
  label?: string;
  icon?: IconName;
  value: string;
  options: readonly SelectOption[];
  onChange: (v: string) => void;
  align?: "left" | "right";
  active?: boolean;
  compact?: boolean;
  minWidth?: number;
  disabled?: boolean;
  className?: string;
  ariaLabel?: string;
}

export function SignalSelectMenu({
  label,
  icon,
  value,
  options,
  onChange,
  align = "left",
  active = false,
  compact = false,
  minWidth,
  disabled = false,
  className,
  ariaLabel,
}: SignalSelectMenuProps) {
  const { open, setOpen, openMenu, triggerRef, menuRef, pos } = useSignalMenu(align);
  const [activeIndex, setActiveIndex] = useState(-1);
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const selectedIndex = options.findIndex((option) => option.value === value);
  const selected = options[selectedIndex] ?? options[0];
  const enabledIndexes = options.flatMap((option, index) =>
    option.disabled ? [] : [index],
  );
  const listboxId = `signal-select-${(ariaLabel ?? label ?? "menu")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")}-listbox`;

  function defaultActiveIndex(direction: 1 | -1 = 1) {
    if (selectedIndex >= 0 && !options[selectedIndex]?.disabled) {
      return selectedIndex;
    }
    return direction === -1
      ? enabledIndexes[enabledIndexes.length - 1] ?? -1
      : enabledIndexes[0] ?? -1;
  }

  function adjacentEnabledIndex(index: number, direction: 1 | -1) {
    if (enabledIndexes.length === 0) return -1;
    const enabledPosition = enabledIndexes.indexOf(index);
    if (enabledPosition === -1) return defaultActiveIndex(direction);
    const nextPosition =
      (enabledPosition + direction + enabledIndexes.length) % enabledIndexes.length;
    return enabledIndexes[nextPosition] ?? -1;
  }

  function closeAndFocusTrigger() {
    setOpen(false);
    triggerRef.current?.focus();
  }

  function choose(index: number) {
    const option = options[index];
    if (!option || option.disabled) return;
    onChange(option.value);
    closeAndFocusTrigger();
  }

  function openAt(index: number) {
    if (index === -1) return;
    setActiveIndex(index);
    openMenu();
  }

  function setActiveAndFocus(index: number) {
    setActiveIndex(index);
    if (index !== -1) optionRefs.current[index]?.focus();
  }

  function moveActive(direction: 1 | -1) {
    const nextIndex = adjacentEnabledIndex(
      activeIndex === -1 ? defaultActiveIndex(direction) : activeIndex,
      direction,
    );
    setActiveAndFocus(nextIndex);
  }

  function onTriggerKeyDown(event: ReactKeyboardEvent<HTMLButtonElement>) {
    if (disabled) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (open) moveActive(1);
      else openAt(defaultActiveIndex(1));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      if (open) moveActive(-1);
      else openAt(defaultActiveIndex(-1));
    } else if (event.key === "Home") {
      event.preventDefault();
      openAt(enabledIndexes[0] ?? -1);
    } else if (event.key === "End") {
      event.preventDefault();
      openAt(enabledIndexes[enabledIndexes.length - 1] ?? -1);
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (open) choose(activeIndex === -1 ? defaultActiveIndex(1) : activeIndex);
      else openAt(defaultActiveIndex(1));
    } else if (event.key === "Escape" && open) {
      event.preventDefault();
      closeAndFocusTrigger();
    }
  }

  function onOptionKeyDown(
    event: ReactKeyboardEvent<HTMLButtonElement>,
    index: number,
  ) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      moveActive(1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      moveActive(-1);
    } else if (event.key === "Home") {
      event.preventDefault();
      setActiveAndFocus(enabledIndexes[0] ?? -1);
    } else if (event.key === "End") {
      event.preventDefault();
      setActiveAndFocus(enabledIndexes[enabledIndexes.length - 1] ?? -1);
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      choose(index);
    } else if (event.key === "Escape") {
      event.preventDefault();
      closeAndFocusTrigger();
    }
  }

  useEffect(() => {
    if (!open) {
      setActiveIndex(-1);
      return;
    }
    setActiveIndex((index) => {
      if (index >= 0 && !options[index]?.disabled) return index;
      return defaultActiveIndex(1);
    });
  }, [open, options, selectedIndex]);

  useEffect(() => {
    if (!open || activeIndex === -1) return;
    window.requestAnimationFrame(() => optionRefs.current[activeIndex]?.focus());
  }, [open, activeIndex]);

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listboxId : undefined}
        aria-label={ariaLabel}
        disabled={disabled}
        onClick={() => {
          if (open) setOpen(false);
          else openAt(defaultActiveIndex(1));
        }}
        onKeyDown={onTriggerKeyDown}
        data-active={active || undefined}
        style={minWidth ? { minWidth } : undefined}
        className={[
          "relative inline-flex items-center gap-1.5 h-8 px-2.5",
          "bg-surface-elev border border-border rounded-sm text-text-2",
          "text-[12.5px] cursor-pointer transition-colors whitespace-nowrap",
          "hover:border-text-3 disabled:hover:border-border focus:outline-none focus:border-gold/45 focus-visible:ring-1 focus-visible:ring-gold/45",
          active ? "border-gold/45 bg-gold/10" : "",
          disabled ? "cursor-not-allowed opacity-50" : "",
          className ?? "",
        ].join(" ")}
      >
        {icon && (
          <Icon name={icon} size={12} className={active ? "text-gold" : "text-text-3"} />
        )}
        {!compact && label && (
          <span className="text-text-3 text-[11.5px] tracking-wide">{label}</span>
        )}
        <span className={`font-medium text-[12.5px] ${active ? "text-gold" : "text-text"}`}>
          {selected?.label}
        </span>
        <Icon name="chevR" size={11} className="text-text-3" />
      </button>
      <MenuShell
        open={open}
        menuRef={menuRef as RefObject<HTMLDivElement>}
        pos={pos}
        minWidth={200}
        role="listbox"
        id={listboxId}
      >
        <div role="presentation">
          {options.map((opt, index) => {
            const isSelected = opt.value === value;
            const isActive = index === activeIndex;
            return (
              <button
                key={opt.value}
                ref={(node) => {
                  optionRefs.current[index] = node;
                }}
                type="button"
                role="option"
                aria-selected={isSelected}
                disabled={opt.disabled}
                onMouseEnter={() => {
                  if (!opt.disabled) setActiveIndex(index);
                }}
                onKeyDown={(event) => onOptionKeyDown(event, index)}
                onClick={() => choose(index)}
                className={[
                  "flex w-full items-center gap-2 px-3 h-[34px] text-[13px] text-text transition-colors disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-transparent",
                  isSelected ? "bg-gold/10" : isActive ? "bg-surface-hover" : "hover:bg-surface-hover",
                ].join(" ")}
              >
                <span className="w-[14px] flex-shrink-0">
                  {isSelected && (
                    <svg width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-gold" aria-hidden>
                      <path d="M4 10l4 4 8-8" />
                    </svg>
                  )}
                </span>
                <span className="flex-1 text-left">{opt.label}</span>
              </button>
            );
          })}
        </div>
      </MenuShell>
    </>
  );
}

// ─── SignalSearchableSelectMenu ───────────────────────────────────────────────
// Searchable single-select dropdown for entity pickers with larger option sets.

export interface SearchableSelectOption {
  value: string;
  label: string;
  meta?: string;
  searchText?: string;
  disabled?: boolean;
  group?: string;
  badge?: string;
}

export interface SignalSearchableSelectMenuProps {
  label?: string;
  ariaLabel: string;
  value: string;
  options: SearchableSelectOption[];
  onChange: (value: string) => void;
  placeholder?: string;
  searchPlaceholder?: string;
  emptyHint?: string;
  loading?: boolean;
  disabled?: boolean;
  align?: "left" | "right";
  className?: string;
  minWidth?: number;
}

export function SignalSearchableSelectMenu({
  label,
  ariaLabel,
  value,
  options,
  onChange,
  placeholder = "Select…",
  searchPlaceholder,
  emptyHint = "No options match",
  loading = false,
  disabled = false,
  align = "left",
  className,
  minWidth = 280,
}: SignalSearchableSelectMenuProps) {
  const { open, setOpen, toggle, triggerRef, menuRef, pos } = useSignalMenu(align);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(-1);
  const filterRef = useRef<HTMLInputElement>(null);
  const selected = options.find((option) => option.value === value);
  const normalized = query.trim().toLowerCase();
  const filtered = normalized
    ? options.filter((option) =>
        (option.searchText ?? `${option.label} ${option.meta ?? ""} ${option.value}`)
          .toLowerCase()
          .includes(normalized),
      )
    : options;
  const enabled = filtered.filter((option) => !option.disabled);
  const listboxId = `signal-searchable-${ariaLabel.toLowerCase().replace(/[^a-z0-9]+/g, "-")}-listbox`;

  useEffect(() => {
    if (open) {
      window.requestAnimationFrame(() => filterRef.current?.focus());
    } else {
      setQuery("");
      setActiveIndex(-1);
    }
  }, [open]);

  useEffect(() => {
    setActiveIndex(-1);
  }, [query]);

  function closeAndFocusTrigger() {
    setOpen(false);
    triggerRef.current?.focus();
  }

  function choose(option: SearchableSelectOption | undefined) {
    if (!option || option.disabled) return;
    onChange(option.value);
    closeAndFocusTrigger();
  }

  function onKeyDown(event: ReactKeyboardEvent<HTMLElement>) {
    if (!open) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActiveIndex((index) => Math.min(index + 1, Math.max(enabled.length - 1, 0)));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActiveIndex((index) => Math.max(index - 1, 0));
    } else if (event.key === "Enter") {
      event.preventDefault();
      choose(enabled[activeIndex] ?? enabled[0]);
    } else if (event.key === "Escape") {
      closeAndFocusTrigger();
    }
  }

  function onOptionKeyDown(
    event: ReactKeyboardEvent<HTMLButtonElement>,
    option: SearchableSelectOption,
  ) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeAndFocusTrigger();
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      choose(option);
    } else {
      onKeyDown(event);
    }
  }

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listboxId : undefined}
        aria-label={ariaLabel}
        disabled={disabled || loading}
        onClick={toggle}
        onKeyDown={onKeyDown}
        className={[
          "inline-flex h-8 min-w-0 items-center gap-1.5 rounded-sm border border-border bg-surface-elev px-2.5 text-[12.5px] text-text transition-colors",
          "hover:border-text-3 focus:outline-none focus:border-gold/45 focus-visible:ring-1 focus-visible:ring-gold/45",
          "disabled:cursor-not-allowed disabled:opacity-50",
          className ?? "",
        ].join(" ")}
      >
        {label ? (
          <span className="text-[11.5px] tracking-wide text-text-3">
            {label}
          </span>
        ) : null}
        <span className="min-w-0 flex-1 truncate text-left font-mono text-[12px]">
          {loading ? "Loading…" : selected?.label ?? placeholder}
        </span>
        <Icon name="chevR" size={11} className="text-text-3" />
      </button>
      <MenuShell
        open={open}
        menuRef={menuRef as RefObject<HTMLDivElement>}
        pos={pos}
        minWidth={minWidth}
        role="menu"
      >
        <div className="border-b border-border px-2 pb-1 pt-2">
          <div className="flex h-8 items-center gap-2 rounded-sm border border-border bg-surface-elev px-2 focus-within:border-gold-soft">
            <Icon name="search" size={13} className="shrink-0 text-text-3" />
            <input
              ref={filterRef}
              type="text"
              aria-label={`Search ${ariaLabel}`}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={onKeyDown}
              placeholder={searchPlaceholder ?? `Search ${ariaLabel.toLowerCase()}…`}
              spellCheck={false}
              className="min-w-0 flex-1 border-none bg-transparent p-0 font-mono text-[12px] text-text outline-none placeholder:text-text-3"
            />
          </div>
        </div>
        <div
          id={listboxId}
          role="listbox"
          aria-label={`${ariaLabel} options`}
          className="max-h-[320px] overflow-y-auto py-1"
        >
          {filtered.length === 0 ? (
            <div className="px-3 py-3 font-mono text-[12px] text-text-3">
              {emptyHint}
            </div>
          ) : (
            filtered.map((option) => {
              const isSelected = option.value === value;
              const enabledIndex = enabled.findIndex((item) => item.value === option.value);
              const isActive = enabledIndex >= 0 && enabledIndex === activeIndex;
              return (
                <button
                  key={option.value}
                  type="button"
                  role="option"
                  aria-selected={isSelected}
                  disabled={option.disabled}
                  onFocus={() => setActiveIndex(enabledIndex)}
                  onMouseEnter={() => setActiveIndex(enabledIndex)}
                  onKeyDown={(event) => onOptionKeyDown(event, option)}
                  onClick={() => choose(option)}
                  className={[
                    "flex min-h-[34px] w-full items-center gap-2 px-3 text-left text-[13px] transition-colors disabled:cursor-not-allowed disabled:opacity-50",
                    isSelected ? "bg-gold/10" : isActive ? "bg-surface-hover" : "hover:bg-surface-hover",
                  ].join(" ")}
                >
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-text">{option.label}</span>
                    {option.meta ? (
                      <span className="block truncate font-mono text-[11px] text-text-3">
                        {option.meta}
                      </span>
                    ) : null}
                  </span>
                  {option.badge ? (
                    <span className="shrink-0 rounded border border-amber-200 bg-amber-50 px-1.5 text-[11px] text-amber-600 dark:border-amber-500/30 dark:bg-amber-950/40 dark:text-amber-400">
                      {option.badge}
                    </span>
                  ) : null}
                  {isSelected ? (
                    <Icon name="check" size={12} className="shrink-0 text-gold" />
                  ) : null}
                </button>
              );
            })
          )}
        </div>
      </MenuShell>
    </>
  );
}

// ─── SignalCheckboxMenu ───────────────────────────────────────────────────────
// Multi-select with colored dots matching filter pill styling.

export interface CheckboxOption {
  value: string;
  label: string;
  dotColor?: string;
  dotVariant?: "filled" | "ring";
}

export interface SignalCheckboxMenuProps {
  label?: string;
  icon?: IconName;
  selected: string[];
  options: CheckboxOption[];
  onChange: (selected: string[]) => void;
  onClear?: () => void;
  align?: "left" | "right";
  triggerLabel?: string;
}

export function SignalCheckboxMenu({
  label,
  icon,
  selected,
  options,
  onChange,
  onClear,
  align = "left",
  triggerLabel,
}: SignalCheckboxMenuProps) {
  const { open, setOpen, toggle, triggerRef, menuRef, pos } = useSignalMenu(align);
  const activeCount = selected.length;

  function toggle2(value: string) {
    if (selected.includes(value)) {
      onChange(selected.filter((v) => v !== value));
    } else {
      onChange([...selected, value]);
    }
  }

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-multiselectable
        onClick={toggle}
        data-active={activeCount > 0 || undefined}
        className={[
          "inline-flex items-center gap-1.5 h-8 px-2.5",
          "bg-surface-elev border border-border rounded-sm text-text-2",
          "text-[12.5px] cursor-pointer transition-colors whitespace-nowrap",
          "hover:border-text-3 focus:outline-none focus:border-gold/45 focus-visible:ring-1 focus-visible:ring-gold/45",
          activeCount > 0 ? "border-gold/45 bg-gold/10" : "",
        ].join(" ")}
      >
        {icon && (
          <Icon name={icon} size={12} className={activeCount > 0 ? "text-gold" : "text-text-3"} />
        )}
        {label && (
          <span className={`text-[11.5px] tracking-wide ${activeCount > 0 ? "text-gold" : "text-text-3"}`}>
            {label}
          </span>
        )}
        {triggerLabel && <span className="font-medium">{triggerLabel}</span>}
        {activeCount > 0 && (
          <span className="inline-flex items-center justify-center px-1.5 h-4 rounded-[2px] bg-gold/20 font-mono text-[10px] text-gold">
            {activeCount}
          </span>
        )}
        <Icon name="chevR" size={11} className="text-text-3" />
      </button>
      <MenuShell open={open} menuRef={menuRef as RefObject<HTMLDivElement>} pos={pos} minWidth={200} role="listbox">
        {options.map((opt) => {
          const isSelected = selected.includes(opt.value);
          return (
            <button
              key={opt.value}
              type="button"
              role="option"
              aria-selected={isSelected}
              onClick={() => toggle2(opt.value)}
              className="flex w-full items-center gap-2.5 px-3 h-[34px] text-[13px] text-text hover:bg-surface-hover transition-colors"
            >
              <span className="w-[14px] flex-shrink-0 flex items-center justify-center">
                {isSelected ? (
                  <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden>
                    <rect width="14" height="14" rx="2" className="fill-gold" />
                    <path d="M3 7l3 3 5-5" stroke="#000" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
                  </svg>
                ) : (
                  <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden>
                    <rect x="0.5" y="0.5" width="13" height="13" rx="1.5" className="stroke-border-strong" />
                  </svg>
                )}
              </span>
              {opt.dotColor && (
                <span
                  className="flex-shrink-0"
                  style={{
                    width: 6,
                    height: 6,
                    borderRadius: "50%",
                    background: opt.dotVariant === "ring" ? "transparent" : opt.dotColor,
                    border: `1px solid ${opt.dotColor}`,
                  }}
                />
              )}
              <span className="flex-1 text-left">{opt.label}</span>
            </button>
          );
        })}
        {onClear && (
          <>
            <SignalMenuSeparator />
            <button
              type="button"
              onClick={() => {
                onClear();
                setOpen(false);
              }}
              className="flex w-full items-center gap-2.5 px-3 h-[34px] text-[12px] text-text-3 hover:text-text hover:bg-surface-hover transition-colors"
            >
              Clear all
            </button>
          </>
        )}
      </MenuShell>
    </>
  );
}

// ─── SignalModelPickerMenu ────────────────────────────────────────────────────
// Filterable model picker grouped by provider with context window display.

function shortModelId(model: string): string {
  const lastSlash = model.lastIndexOf("/");
  return lastSlash >= 0 ? model.slice(lastSlash + 1) : model;
}

export interface ModelOption {
  provider: string;
  model: string;
  contextWindow?: string;
}

export interface SignalModelPickerMenuProps {
  options: ModelOption[];
  provider: string | null;
  model: string;
  onChange: (provider: string | null, model: string) => void;
  loading?: boolean;
  align?: "left" | "right";
  placeholder?: string;
  /** Layout/width classes appended to the trigger button (e.g. `w-full`,
   *  `flex-1 min-w-0`). The trigger keeps its own visual styling. */
  className?: string;
  /** Accessible name for the trigger button. */
  ariaLabel?: string;
  /** Message shown in the menu body when there are no options at all
   *  (e.g. no providers configured). */
  emptyHint?: string;
}

export function SignalModelPickerMenu({
  options,
  provider,
  model,
  onChange,
  loading,
  align = "left",
  placeholder = "— pick a model —",
  className,
  ariaLabel,
  emptyHint,
}: SignalModelPickerMenuProps) {
  const { open, setOpen, toggle, triggerRef, menuRef, pos } = useSignalMenu(align);
  const [filter, setFilter] = useState("");
  const filterRef = useRef<HTMLInputElement>(null);

  const selectedLabel =
    provider && model ? shortModelId(model) : placeholder;

  // Focus filter input when menu opens
  useEffect(() => {
    if (open && filterRef.current) {
      filterRef.current.focus();
    }
    if (!open) setFilter("");
  }, [open]);

  const filtered = filter
    ? options.filter(
        (o) =>
          o.model.toLowerCase().includes(filter.toLowerCase()) ||
          o.provider.toLowerCase().includes(filter.toLowerCase()),
      )
    : options;

  // Group by provider
  const groups = filtered.reduce<Record<string, ModelOption[]>>((acc, o) => {
    if (!acc[o.provider]) acc[o.provider] = [];
    acc[o.provider].push(o);
    return acc;
  }, {});

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={ariaLabel}
        disabled={loading}
        onClick={toggle}
        className={[
          "inline-flex items-center gap-1.5 h-8 px-2.5 bg-surface-elev border border-border rounded-sm text-text text-[12.5px] cursor-pointer transition-colors overflow-hidden min-w-0 whitespace-nowrap hover:border-text-3 focus:outline-none focus:border-gold/45 focus-visible:ring-1 focus-visible:ring-gold/45 disabled:opacity-50",
          className ?? "",
        ].join(" ")}
      >
        <span className="flex-1 min-w-0 text-left font-mono text-[12px] truncate">
          {loading ? "Loading…" : selectedLabel}
        </span>
        <Icon name="chevR" size={11} className="text-text-3" />
      </button>
      <MenuShell open={open} menuRef={menuRef as RefObject<HTMLDivElement>} pos={pos} minWidth={260} role="listbox">
        {/* Filter input */}
        <div className="px-2 pt-2 pb-1 border-b border-border">
          <div className="flex items-center gap-2 px-2 h-8 bg-surface-elev border border-border rounded-sm focus-within:border-gold-soft">
            <Icon name="search" size={13} className="text-text-3 flex-shrink-0" />
            <input
              ref={filterRef}
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="Filter models…"
              spellCheck={false}
              className="flex-1 min-w-0 bg-transparent border-none outline-none text-text text-[12px] p-0 placeholder:text-text-3 font-mono"
            />
          </div>
        </div>
        {/* Provider groups */}
        <div className="max-h-[320px] overflow-y-auto py-1">
          {options.length > 0 && placeholder && (
            <button
              type="button"
              role="option"
              aria-selected={!provider && !model}
              onClick={() => {
                onChange(null, "");
                setOpen(false);
              }}
              className={[
                "flex w-full items-center gap-2 px-3 h-[34px] transition-colors",
                !provider && !model ? "bg-gold/10" : "hover:bg-surface-hover",
              ].join(" ")}
            >
              <span className="flex-1 text-left font-mono text-[12px] text-text-3">
                {placeholder}
              </span>
              {!provider && !model && (
                <svg width="12" height="12" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" className="text-gold flex-shrink-0" aria-hidden>
                  <path d="M4 10l4 4 8-8" />
                </svg>
              )}
            </button>
          )}
          {Object.entries(groups).length === 0 && (
            <div className="px-3 py-3 text-[12px] text-text-3 font-mono">
              {options.length === 0 ? (emptyHint ?? "No models available") : "No models match"}
            </div>
          )}
          {Object.entries(groups).map(([prov, models]) => (
            <div key={prov}>
              <SignalMenuLabel>{prov}</SignalMenuLabel>
              {models.map((o) => {
                const isSelected = o.provider === provider && o.model === model;
                return (
                  <button
                    key={`${o.provider}::${o.model}`}
                    type="button"
                    role="option"
                    aria-selected={isSelected}
                    onClick={() => {
                      onChange(o.provider, o.model);
                      setOpen(false);
                    }}
                    className={[
                      "flex w-full items-center gap-2 px-3 h-[34px] transition-colors",
                      isSelected ? "bg-gold/10" : "hover:bg-surface-hover",
                    ].join(" ")}
                  >
                    <span className="flex-1 text-left font-mono text-[12px] text-text">
                      {o.model}
                    </span>
                    {o.contextWindow && (
                      <span className="font-mono text-[11px] text-text-3">{o.contextWindow}</span>
                    )}
                    {isSelected && (
                      <svg width="12" height="12" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" className="text-gold flex-shrink-0" aria-hidden>
                        <path d="M4 10l4 4 8-8" />
                      </svg>
                    )}
                  </button>
                );
              })}
            </div>
          ))}
        </div>
      </MenuShell>
    </>
  );
}
