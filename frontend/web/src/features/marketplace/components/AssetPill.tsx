// src/features/marketplace/components/AssetPill.tsx
const TONE: Record<string, string> = {
  BTC: "text-[#FBBF24] bg-[#FBBF24]/10 border-[#FBBF24]/20",
  ETH: "text-info bg-info/10 border-info/20",
  SOL: "text-[#A78BFA] bg-[#A78BFA]/10 border-[#A78BFA]/20",
  DOGE: "text-[#F472B6] bg-[#F472B6]/10 border-[#F472B6]/20",
};
const FALLBACK = "text-text-2 bg-surface-elev border-border";

export function AssetPill({ asset }: { asset: string }) {
  return (
    <span
      className={`inline-flex items-center px-1.5 py-0.5 rounded-sm border text-[10px] font-medium tracking-wide ${TONE[asset] ?? FALLBACK}`}
    >
      {asset}
    </span>
  );
}
