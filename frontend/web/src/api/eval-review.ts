// Eval-review API — typed wrappers around the dashboard's
// `/api/eval/runs/:id/review[s]` and `/api/eval/reviews/:id` routes.
//
// The engine derives ts-rs types for `EvalReview` / `AgentProfile` etc.
// but the generated types.gen/ files haven't been regenerated yet; until
// they are, the request/response shapes live here as hand-rolled types.
// Mirrors `xvision_engine::eval::review::*` exactly.

import { apiFetch } from "./client";
import type { Finding } from "./types.gen";

export type ReviewStatus = "queued" | "running" | "completed" | "failed";
export type ReviewVerdict = "promising" | "weak" | "failed" | "inconclusive";

export type EvalReview = {
  id: string;
  eval_run_id: string;
  agent_profile_id: string;
  status: ReviewStatus;
  verdict: ReviewVerdict | null;
  confidence: number | null;
  score: number | null;
  summary: string | null;
  raw_output_json: string | null;
  error: string | null;
  created_at: string;
  updated_at: string;
};

/// Review-linked v2 columns aren't in the generated `Finding` type yet
/// (types.gen needs regenerating). We widen the shape via `Omit` +
/// intersection so callers can read the review-only fields, and we also
/// widen `severity` to accept the v2 vocab (`low | medium | high |
/// critical`) on top of the v1 vocab (`info | warning | critical`). The
/// v1 extractor and the v2 review write to the same `eval_findings`
/// table, but with different severity tags.
export type ReviewSeverity = "low" | "medium" | "high" | "critical" | "info" | "warning";
export type ReviewFinding = Omit<Finding, "severity"> & {
  severity: ReviewSeverity;
  eval_review_id?: string;
  type?: string;
  confidence?: number;
  title?: string;
  description?: string;
  recommendation?: string;
  created_at?: string;
};

export type ReviewDetail = {
  review: EvalReview;
  findings: ReviewFinding[];
};

export type ReviewListResponse = {
  items: EvalReview[];
};

export type GenerateReviewBody = {
  agent_profile_id: string;
  force?: boolean;
};

export type AgentProfile = {
  id: string;
  name: string;
  type: string;
  provider: string;
  model: string;
  temperature: number;
  max_tokens: number;
  system_prompt: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
};

export const reviewKeys = {
  all: ["eval-review"] as const,
  forRun: (runId: string) => [...reviewKeys.all, "for-run", runId] as const,
  detail: (reviewId: string) => [...reviewKeys.all, "detail", reviewId] as const,
};

export function listReviewsForRun(runId: string): Promise<EvalReview[]> {
  return apiFetch<ReviewListResponse>(
    `/api/eval/runs/${encodeURIComponent(runId)}/reviews`,
  ).then((r) => r.items);
}

export function getReview(reviewId: string): Promise<ReviewDetail> {
  return apiFetch<ReviewDetail>(
    `/api/eval/reviews/${encodeURIComponent(reviewId)}`,
  );
}

export function generateReview(
  runId: string,
  body: GenerateReviewBody,
): Promise<ReviewDetail> {
  return apiFetch<ReviewDetail>(
    `/api/eval/runs/${encodeURIComponent(runId)}/review`,
    {
      method: "POST",
      body: JSON.stringify(body),
    },
  );
}

export const agentProfileKeys = {
  all: ["agent-profiles"] as const,
  list: () => [...agentProfileKeys.all, "list"] as const,
};

/// `GET /api/eval/agent-profiles` — list the seeded review profiles
/// with their current provider/model assignment. Used by the agent
/// picker so the operator sees what each profile will actually
/// dispatch to (and can change it).
export function listAgentProfiles(): Promise<AgentProfile[]> {
  return apiFetch<{ items: AgentProfile[] }>("/api/eval/agent-profiles").then(
    (r) => r.items,
  );
}

/// `PATCH /api/eval/agent-profiles/:id` — reseat a profile against a
/// different provider/model. Backend validates that `provider` (when
/// supplied) is a name present in `$XVN_HOME/config/default.toml`;
/// passing an unknown name surfaces an `ApiError` with `code:
/// "validation"`.
export function updateAgentProfile(
  id: string,
  body: {
    provider?: string;
    model?: string;
    temperature?: number;
    max_tokens?: number;
    system_prompt?: string;
  },
): Promise<AgentProfile> {
  return apiFetch<AgentProfile>(
    `/api/eval/agent-profiles/${encodeURIComponent(id)}`,
    {
      method: "PATCH",
      body: JSON.stringify(body),
    },
  );
}

export type CriticalFinding = ReviewFinding & {
  runId: string;
  strategyName?: string;
};

export async function listCriticalFindings(
  runs: Array<{ id: string; strategy?: { display_name?: string } | null }>,
  opts?: { maxRuns?: number; maxFindings?: number },
): Promise<CriticalFinding[]> {
  const maxRuns = opts?.maxRuns ?? 3;
  const maxFindings = opts?.maxFindings ?? 5;

  // Take the first maxRuns runs
  const targetRuns = runs.slice(0, maxRuns);

  // Fan out: for each run, get its reviews
  const results = await Promise.all(
    targetRuns.map(async (run) => {
      try {
        const reviews = await listReviewsForRun(run.id);
        // Find the first completed review
        const completed = reviews.find((r) => r.status === "completed");
        if (!completed) return [];
        // Get findings
        const detail = await getReview(completed.id);
        // Filter to critical only
        return detail.findings
          .filter((f) => f.severity === "critical")
          .map(
            (f): CriticalFinding => ({
              ...f,
              runId: run.id,
              strategyName: run.strategy?.display_name ?? undefined,
            }),
          );
      } catch {
        return [];
      }
    }),
  );

  // Flatten, sort by created_at desc (if available), cap at maxFindings
  return results
    .flat()
    .sort((a, b) => {
      if (!a.created_at || !b.created_at) return 0;
      return b.created_at.localeCompare(a.created_at);
    })
    .slice(0, maxFindings);
}

/// Canonical review-agent profile ids seeded by migration 016.
/// Used as a static fallback for label/blurb metadata that isn't
/// stored on the AgentProfile row itself.
export const CANONICAL_AGENT_PROFILES: ReadonlyArray<{
  id: string;
  label: string;
  blurb: string;
}> = [
  {
    id: "fast-trader-agent",
    label: "Fast Trader",
    blurb: "Quick tactical read; obvious pass/fail.",
  },
  {
    id: "reasoning-agent",
    label: "Reasoning",
    blurb: "Evidence-backed causal analysis.",
  },
  {
    id: "risk-agent",
    label: "Risk",
    blurb: "Tail risk, drawdown, robustness.",
  },
  {
    id: "research-agent",
    label: "Research",
    blurb: "Next experiments and mutations.",
  },
];
