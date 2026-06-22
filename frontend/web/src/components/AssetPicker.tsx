import type { AssetInfo } from "@/api/assets";
import { SignalSearchableSelectMenu } from "@/components/primitives/SignalMenu";

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
  return (
    <SignalSearchableSelectMenu
      ariaLabel="Asset picker"
      value={value}
      options={assets.map((asset) => ({
        value: asset.symbol,
        label: asset.symbol,
        meta: asset.category,
        searchText: `${asset.symbol} ${asset.category} ${asset.data}`,
        badge:
          showOrderlyOnlyBadge && asset.data === "orderly-only"
            ? "no backtest data"
            : undefined,
      }))}
      onChange={onChange}
      placeholder={placeholder}
      searchPlaceholder={placeholder}
      emptyHint="No assets found"
      className={className}
      minWidth={280}
    />
  );
}
