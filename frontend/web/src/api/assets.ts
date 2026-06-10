import { useQuery } from "@tanstack/react-query";

export interface AssetInfo {
  symbol: string;
  category: string;
  data: string; // "alpaca" | "orderly-only"
  venues: Record<string, string>;
  enabled: boolean;
}

async function fetchAssets(): Promise<AssetInfo[]> {
  const res = await fetch("/api/assets");
  if (!res.ok) throw new Error(`GET /api/assets: ${res.status}`);
  return res.json();
}

export function useAssets() {
  return useQuery({
    queryKey: ["assets"],
    queryFn: fetchAssets,
    staleTime: 5 * 60 * 1000,
  });
}

/** Returns assets usable in backtest/scenario contexts (have Alpaca bar data). */
export function useAlpacaAssets() {
  const q = useAssets();
  return {
    ...q,
    data: q.data?.filter((a) => a.data === "alpaca" && a.enabled) ?? [],
  };
}
