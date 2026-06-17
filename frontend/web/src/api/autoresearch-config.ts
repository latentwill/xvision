// Autoresearch configuration API.
// Routes backed by the config store on the server:
//   GET  /api/settings/autoresearch   → AutoresearchConfig (all 8 keys)
//   POST /api/settings/autoresearch   → AutoresearchConfig (partial update, returns full)
//
// Wire shape mirrors `AutoresearchConfigResponse` in
// crates/xvision-dashboard/src/routes/settings_autoresearch.rs.

import { apiFetch } from "@/api/client";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";

export type AutoresearchConfig = {
  /** Minimum precision lift in percentage points to consider a checkpoint successful. Default 3.0. */
  min_precision_lift_pp: number;
  /** Maximum allowed PnL regression vs baseline (0 = non-negative). Default 0.0. */
  max_pnl_regression: number;
  /** val_acc must beat current best by this margin to promote. Default 0.01. */
  promotion_epsilon: number;
  /** Absolute val_acc floor for promotion. Default 0.52. Range 0–1. */
  promotion_acc_floor: number;
  /** Minimum holdout_samples for promotion. Default 200. */
  promotion_min_holdout: number;
  /** Minimum cycle count required to start a run. Default 500. */
  min_cycle_count: number;
  /** Wall-clock budget per training subprocess (seconds). Default 300. */
  train_wall_clock_sec: number;
  /** Price forward threshold for data quality gate. Default 0.003. */
  price_forward_threshold: number;
};

export const autoresearchConfigKeys = {
  all: ["autoresearch-config"] as const,
  config: () => [...autoresearchConfigKeys.all, "config"] as const,
};

export function getAutoresearchConfig(): Promise<AutoresearchConfig> {
  return apiFetch<AutoresearchConfig>("/api/settings/autoresearch");
}

// POST body is a partial — only the keys the caller wants to change are sent.
// The server validates ranges and returns the full config after write.
export function setAutoresearchConfig(
  cfg: Partial<AutoresearchConfig>,
): Promise<AutoresearchConfig> {
  return apiFetch<AutoresearchConfig>("/api/settings/autoresearch", {
    method: "POST",
    body: JSON.stringify(cfg),
  });
}

export function useAutoresearchConfig() {
  return useQuery({
    queryKey: autoresearchConfigKeys.config(),
    queryFn: getAutoresearchConfig,
    staleTime: 60_000,
  });
}

export function useSetAutoresearchConfig() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: setAutoresearchConfig,
    onSuccess: (data) => {
      qc.setQueryData(autoresearchConfigKeys.config(), data);
    },
  });
}
