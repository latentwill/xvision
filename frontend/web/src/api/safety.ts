// `/api/safety/*` — safety gate state, pause/resume, audit log.

import { apiFetch } from "./client";

// ── Types ───────────────────────────────────────────────────────────────────

export type VenueLabel = "paper" | "testnet" | "live";

export type SafetyStateResponse = {
  paused: boolean;
  paused_at?: string | null;
  paused_by?: string | null;
  reason?: string | null;
};

export type PauseRequest = {
  reason?: string | null;
};

export type SafetyAuditRow = {
  id: number;
  timestamp: string;
  user: string;
  source: string;
  action_kind: string;
  params_json: string;
  result: string;
  pause_state_at_time: boolean;
};

// ── Query keys ───────────────────────────────────────────────────────────────

export const safetyKeys = {
  all: ["safety"] as const,
  state: () => [...safetyKeys.all, "state"] as const,
  audit: (limit: number) => [...safetyKeys.all, "audit", limit] as const,
};

// ── API functions ─────────────────────────────────────────────────────────────

export function getSafetyState(): Promise<SafetyStateResponse> {
  return apiFetch<SafetyStateResponse>("/api/safety/state");
}

export function pauseSafety(req: PauseRequest = {}): Promise<SafetyStateResponse> {
  return apiFetch<SafetyStateResponse>("/api/safety/pause", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export function resumeSafety(req: PauseRequest = {}): Promise<SafetyStateResponse> {
  return apiFetch<SafetyStateResponse>("/api/safety/resume", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export function getSafetyAudit(limit = 50): Promise<SafetyAuditRow[]> {
  return apiFetch<SafetyAuditRow[]>(`/api/safety/audit?limit=${limit}`);
}
