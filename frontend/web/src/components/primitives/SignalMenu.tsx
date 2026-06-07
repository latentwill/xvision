// Signal floating menu system.
//
// Provides three variants: SignalActionMenu (⋯ overflow), SignalSelectMenu
// (single-select with checkmark), SignalCheckboxMenu (multi-select), and
// useSignalMenu (hook for custom compositions).
//
// All menus render in a portal at document.body, positioned via fixed coords
// relative to their trigger's getBoundingClientRect(). They close on
// click-outside and Escape.

import {
  createPortal,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import type { CSSProperties, ReactNode, RefObject } from "react";
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

  return { open, setOpen, toggle, triggerRef, menuRef, pos };
}

// ─── Menu shell ───────────────────────────────────────────────────────────────

interface MenuShellProps {
  open: boolean;
  menuRef: RefObject<HTMLDivElement | null>;
  pos: CSSProperties;
  children: ReactNode;
  minWidth?: number;
  className?: string;
  role?: "menu" | "listbox";
}

function MenuShell({ open, menuRef, pos, children, minWidth = 220, className, role = "menu" }: MenuShellProps) {
  if (!open) return null;
  return createPortal(
    <div
      ref={menuRef}
      role={role}
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
        className={
          triggerClassName ??
          "inline-flex h-7 w-7 items-center justify-center rounded text-text-3 transition-colors hover:bg-surface-hover hover:text-text"
        }
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
}

export interface SignalSelectMenuProps {
  label?: string;
  icon?: IconName;
  value: string;
  options: SelectOption[];
  onChange: (v: string) => void;
  align?: "left" | "right";
  active?: boolean;
  compact?: boolean;
  minWidth?: number;
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
}: SignalSelectMenuProps) {
  const { open, setOpen, toggle, triggerRef, menuRef, pos } = useSignalMenu(align);
  const selected = options.find((o) => o.value === value) ?? options[0];

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={toggle}
        data-active={active || undefined}
        style={minWidth ? { minWidth } : undefined}
        className={[
          "relative inline-flex items-center gap-1.5 h-8 px-2.5",
          "bg-surface-elev border border-border rounded-sm text-text-2",
          "text-[12.5px] cursor-pointer transition-colors whitespace-nowrap",
          "hover:border-text-3",
          active ? "border-gold/45 bg-gold/10" : "",
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
      <MenuShell open={open} menuRef={menuRef as RefObject<HTMLDivElement>} pos={pos} minWidth={200} role="listbox">
        {options.map((opt) => {
          const isSelected = opt.value === value;
          return (
            <button
              key={opt.value}
              type="button"
              role="option"
              aria-selected={isSelected}
              onClick={() => {
                onChange(opt.value);
                setOpen(false);
              }}
              className="flex w-full items-center gap-2 px-3 h-[34px] text-[13px] text-text hover:bg-surface-hover transition-colors"
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
          "hover:border-text-3",
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
}

export function SignalModelPickerMenu({
  options,
  provider,
  model,
  onChange,
  loading,
  align = "left",
  placeholder = "— pick a model —",
}: SignalModelPickerMenuProps) {
  const { open, setOpen, toggle, triggerRef, menuRef, pos } = useSignalMenu(align);
  const [filter, setFilter] = useState("");
  const filterRef = useRef<HTMLInputElement>(null);

  const selectedLabel =
    provider && model ? `${model}` : placeholder;

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
        disabled={loading}
        onClick={toggle}
        className="inline-flex items-center gap-1.5 h-8 px-2.5 bg-surface-elev border border-border rounded-sm text-text text-[12.5px] cursor-pointer transition-colors whitespace-nowrap hover:border-text-3 disabled:opacity-50"
      >
        <span className="flex-1 text-left font-mono text-[12px]">
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
          {Object.entries(groups).length === 0 && (
            <div className="px-3 py-3 text-[12px] text-text-3 font-mono">No models match</div>
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

