// Pine Script import API — wraps `POST /api/strategy/import/pine`.
// Types mirror the WU7 contract (see plan Part 4, WU7/WU8).

import { apiFetch } from "./client";
import type { Strategy } from "./strategies";

// ─── Fidelity report types ──────────────────────────────────────────────────

export type FidelityItem = {
  item: string;
  reason: string;
};

export type CostModel = {
  commission_type: string;
  commission_value_bps: number;
  slippage_model: string;
  slippage_value_bps: number;
  fill_timing: string;
  note: string;
};

export type FidelityReport = {
  captured: FidelityItem[];
  approximated: FidelityItem[];
  dropped: FidelityItem[];
  cost_model: CostModel;
};

// ─── Request / response ─────────────────────────────────────────────────────

export type PineImportRequest = {
  source: string;
  name?: string;
};

export type PineImportResult = {
  strategy: Strategy;
  fidelity_report: FidelityReport;
};

// ─── API call ───────────────────────────────────────────────────────────────

/**
 * POST /api/strategy/import/pine
 *
 * Accepts a Pine Script source string and an optional strategy name,
 * returns the created Strategy and its FidelityReport on success,
 * or throws ApiError on 400 (parse / validation failure).
 */
export function importPineScript(body: PineImportRequest): Promise<PineImportResult> {
  return apiFetch<PineImportResult>("/api/strategy/import/pine", {
    method: "POST",
    body: JSON.stringify({
      source: body.source,
      ...(body.name !== undefined && body.name.trim().length > 0
        ? { name: body.name.trim() }
        : {}),
    }),
  });
}
