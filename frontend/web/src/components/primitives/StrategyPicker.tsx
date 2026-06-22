import type { StrategyListItem } from "@/api/strategies";
import {
  SignalSearchableSelectMenu,
  type SearchableSelectOption,
} from "./SignalMenu";

export interface StrategyPickerProps {
  strategies: StrategyListItem[];
  value: string;
  onChange: (strategyId: string) => void;
  ariaLabel?: string;
  label?: string;
  placeholder?: string;
  loading?: boolean;
  disabled?: boolean;
  className?: string;
}

export function strategySearchText(strategy: StrategyListItem): string {
  return [
    strategy.display_name,
    strategy.agent_id,
    strategy.bundle_hash,
    strategy.template,
    strategy.origin,
    ...(strategy.tags ?? []),
    ...(strategy.providers ?? []),
    ...(strategy.models ?? []),
    ...(strategy.capabilities ?? []),
    ...(strategy.asset_universe ?? []),
  ]
    .filter(Boolean)
    .join(" ");
}

function toOption(strategy: StrategyListItem): SearchableSelectOption {
  const hash = strategy.bundle_hash ? ` · ${strategy.bundle_hash.slice(0, 12)}` : "";
  const origin = strategy.origin ? ` · ${strategy.origin}` : "";

  return {
    value: strategy.agent_id,
    label: strategy.display_name || "Untitled strategy",
    meta: `${strategy.agent_id}${hash}${origin}`,
    searchText: strategySearchText(strategy),
  };
}

export function StrategyPicker({
  strategies,
  value,
  onChange,
  ariaLabel = "Strategy",
  label,
  placeholder = "— pick a strategy —",
  loading = false,
  disabled = false,
  className,
}: StrategyPickerProps) {
  return (
    <SignalSearchableSelectMenu
      ariaLabel={ariaLabel}
      label={label}
      value={value}
      options={strategies.map(toOption)}
      onChange={onChange}
      placeholder={placeholder}
      searchPlaceholder="Search strategies…"
      emptyHint={
        strategies.length === 0 ? "No strategies available" : "No strategies match"
      }
      loading={loading}
      disabled={disabled}
      className={className}
      minWidth={320}
    />
  );
}
