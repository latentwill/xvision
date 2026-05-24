import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Card } from "@/components/primitives/Card";
import {
  generateReview,
  getReview,
  listReviewsForRun,
  reviewKeys,
} from "@/api/eval-review";
import type { EvalReview, ReviewDetail } from "@/api/eval-review";
import { ApiError } from "@/api/client";
import { AgentPicker } from "./AgentPicker";
import { ReviewContent } from "./ReviewContent";

/// Top-level review panel. Mounts inside `/eval-runs/:id`. Drives:
///
/// 1. Lists existing reviews for the run (newest first).
/// 2. Selects one to display (defaults to the newest non-failed row).
/// 3. Lets the operator generate a fresh review via the agent picker.
/// 4. Surfaces empty / failed / inconclusive states explicitly so the
///    panel doesn't render an empty card when no review exists.
export function ReviewPanel({
  runId,
  runIsCompleted,
}: {
  runId: string;
  runIsCompleted: boolean;
}) {
  const qc = useQueryClient();

  const listQuery = useQuery({
    queryKey: reviewKeys.forRun(runId),
    queryFn: () => listReviewsForRun(runId),
    enabled: runId.length > 0 && runIsCompleted,
  });

  // The selected review id; `null` means "show the newest non-failed one
  // returned by the list query". Set by either the user clicking a
  // history entry or by a successful generate mutation.
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const effectiveId =
    selectedId ?? newestNonFailed(listQuery.data ?? null)?.id ?? null;

  const detailQuery = useQuery({
    queryKey: reviewKeys.detail(effectiveId ?? ""),
    queryFn: () => getReview(effectiveId as string),
    enabled: !!effectiveId,
  });

  const generate = useMutation({
    mutationFn: (agentProfileId: string) =>
      generateReview(runId, { agent_profile_id: agentProfileId, force: true }),
    onSuccess: (detail: ReviewDetail) => {
      qc.setQueryData(reviewKeys.detail(detail.review.id), detail);
      qc.invalidateQueries({ queryKey: reviewKeys.forRun(runId) });
      setSelectedId(detail.review.id);
    },
  });

  if (!runIsCompleted) {
    return (
      <Card className="p-5">
        <PanelHeader />
        <div className="text-text-3 text-[13px] font-medium">
          Reviews are available after the run finishes.
        </div>
      </Card>
    );
  }

  const reviews = listQuery.data ?? [];
  const hasReviews = reviews.length > 0;
  const detail = detailQuery.data ?? null;
  const detailReview: EvalReview | null = detail?.review ?? null;
  const [copiedFormat, setCopiedFormat] = useState<"md" | "json" | null>(null);

  async function copyReview(format: "md" | "json", detail: ReviewDetail) {
    const text =
      format === "md"
        ? reviewDetailToMarkdown(detail)
        : JSON.stringify(detail, null, 2);
    await navigator.clipboard.writeText(text);
    setCopiedFormat(format);
    window.setTimeout(() => setCopiedFormat(null), 1500);
  }

  return (
    <Card className="p-5">
      <PanelHeader />

      {/* Agent picker is always visible — even with a prior review, the
          operator can re-run with a different persona. */}
      <div className="mb-4">
        <div className="text-text-3 text-[12px] mb-2">
          Review with:
        </div>
        <AgentPicker
          selected={generate.variables ?? null}
          busy={generate.isPending}
          onSelect={(id) => generate.mutate(id)}
        />
        {generate.isError && (
          <GenerateErrorAlert
            error={generate.error}
            onRetry={() => {
              const last = generate.variables;
              if (last) generate.mutate(last);
            }}
          />
        )}
      </div>

      {/* History strip — only render when there's more than one prior
          review, otherwise the empty / single-review case carries it. */}
      {hasReviews && reviews.length > 1 && (
        <HistoryStrip
          reviews={reviews}
          selectedId={effectiveId}
          onSelect={setSelectedId}
        />
      )}

      {listQuery.isPending && (
        <div className="text-text-3 text-[13px] font-medium">
          Loading reviews…
        </div>
      )}

      {listQuery.isError && (
        <div
          role="alert"
          className="border border-danger/40 rounded-card p-3 mb-3 text-danger text-[13px]"
        >
          Couldn't load review history:{" "}
          {listQuery.error instanceof Error
            ? listQuery.error.message
            : String(listQuery.error)}
          <button
            type="button"
            onClick={() => listQuery.refetch()}
            className="ml-2 underline decoration-dotted underline-offset-2 text-danger hover:text-danger/80"
          >
            retry
          </button>
        </div>
      )}

      {listQuery.isSuccess && !hasReviews && !generate.isPending && (
        <div className="text-text-2 text-[13px]">
          No review yet for this run. Pick an agent above to generate one.
        </div>
      )}

      {generate.isPending && (
        <div
          className="text-text-2 text-[13px] font-medium"
          role="status"
          aria-live="polite"
        >
          Generating review… this can take 10–30 seconds depending on the
          provider.
        </div>
      )}

      {detailQuery.isPending && effectiveId && (
        <div className="text-text-3 text-[13px] font-medium">Loading review…</div>
      )}

      {detailQuery.isError && effectiveId && (
        <div
          role="alert"
          className="border border-danger/40 rounded-card p-3 mb-3 text-danger text-[13px]"
        >
          Couldn't load review details:{" "}
          {detailQuery.error instanceof Error
            ? detailQuery.error.message
            : String(detailQuery.error)}
          <button
            type="button"
            onClick={() => detailQuery.refetch()}
            className="ml-2 underline decoration-dotted underline-offset-2 text-danger hover:text-danger/80"
          >
            retry
          </button>
        </div>
      )}

      {detail && detailReview && (
        <>
          {detailReview.status === "failed" && (
            <div className="border border-danger/40 rounded-card p-3 mb-3 text-danger text-[13px]">
              Review failed: {detailReview.error ?? "(no detail recorded)"}
            </div>
          )}
          {detailReview.status === "completed" && (
            <>
              <div className="mb-3 flex flex-wrap items-center justify-end gap-2">
                <button
                  type="button"
                  data-testid="copy-review-md"
                  onClick={() => void copyReview("md", detail)}
                  className="rounded-sm border border-border-soft bg-surface-elev px-2.5 py-1 text-[12px] text-text-2 hover:border-gold/40 hover:text-text"
                >
                  {copiedFormat === "md" ? "Copied MD" : "Copy MD"}
                </button>
                <button
                  type="button"
                  data-testid="copy-review-json"
                  onClick={() => void copyReview("json", detail)}
                  className="rounded-sm border border-border-soft bg-surface-elev px-2.5 py-1 text-[12px] text-text-2 hover:border-gold/40 hover:text-text"
                >
                  {copiedFormat === "json" ? "Copied JSON" : "Copy JSON"}
                </button>
              </div>
              <ReviewContent
                review={detailReview}
                findings={detail.findings}
              />
            </>
          )}
          {(detailReview.status === "queued" ||
            detailReview.status === "running") && (
            <div className="text-text-3 text-[13px] font-medium">
              Review in flight ({detailReview.status})…
            </div>
          )}
        </>
      )}
    </Card>
  );
}

function reviewDetailToMarkdown(detail: ReviewDetail): string {
  const { review, findings } = detail;
  const lines: string[] = [
    `# Eval Review ${review.id}`,
    "",
    `- Run: ${review.eval_run_id}`,
    `- Agent profile: ${review.agent_profile_id}`,
    `- Status: ${review.status}`,
    `- Verdict: ${review.verdict ?? "n/a"}`,
    `- Score: ${review.score ?? "n/a"}`,
    `- Confidence: ${review.confidence ?? "n/a"}`,
    `- Updated: ${review.updated_at}`,
  ];
  if (review.summary?.trim()) {
    lines.push("", "## Executive Summary", "", review.summary.trim());
  }
  lines.push("", `## Findings (${findings.length})`);
  if (findings.length === 0) {
    lines.push("", "No findings recorded.");
  } else {
    for (const finding of findings) {
      const title = finding.title || finding.summary || finding.id;
      lines.push("", `### ${title}`, "", `- Severity: ${finding.severity}`);
      if (finding.description) lines.push(`- Description: ${finding.description}`);
      if (finding.recommendation) lines.push(`- Recommendation: ${finding.recommendation}`);
      if (finding.confidence != null) lines.push(`- Confidence: ${finding.confidence}`);
    }
  }
  return `${lines.join("\n")}\n`;
}

/// Render the `generate` mutation's error in a high-visibility alert
/// that surfaces BOTH `error.code` (as a small badge) and
/// `error.message`. Mirrors the visual weight of the list/detail error
/// blocks above so a failed Review-with click no longer reads as
/// "click did nothing".
///
/// The backend emits `{ code, message, field? }` for validation errors
/// (and `{ code, message }` for the rest); `message` is already an
/// operator-actionable string with no server-side jargon prefix —
/// see `crates/xvision-dashboard/src/error.rs`'s `IntoResponse` impl.
/// The structured `error.code` ("validation" / "internal" / "not_found"
/// / "conflict" / "http_error") is rendered as a tag so the operator
/// can distinguish a missing-provider remediation from a transient
/// 500.
export function GenerateErrorAlert({
  error,
  onRetry,
}: {
  error: unknown;
  onRetry?: () => void;
}) {
  const { code, message } = describeReviewError(error);
  const providerRemediation =
    message.includes("review skipped") ||
    message.includes("cannot use provider") ||
    message.includes("Settings → Providers");
  return (
    <div
      role="alert"
      data-testid="review-generate-error"
      className="border border-danger/40 rounded-card p-3 mt-3 text-danger text-[13px]"
    >
      <div className="flex items-center gap-2 mb-1">
        <span
          data-testid="review-generate-error-code"
          className="inline-flex items-center px-1.5 py-0.5 rounded-sm text-[10px] uppercase tracking-wide border border-danger/40 text-danger/90 bg-danger/10"
        >
          {code}
        </span>
        <span className="text-danger/80 text-[12px]">
          Could not generate review
        </span>
      </div>
      <div data-testid="review-generate-error-message">{message}</div>
      {providerRemediation ? (
        <div className="mt-2 rounded-sm border border-danger/25 bg-danger/[0.04] px-2 py-1 text-[12px] text-danger/90">
          Open the gear next to the selected review agent, choose a configured
          provider and enabled model, save it, then generate the review again.
        </div>
      ) : null}
      {onRetry && (
        <button
          type="button"
          onClick={onRetry}
          className="mt-2 underline decoration-dotted underline-offset-2 text-danger hover:text-danger/80 text-[12px]"
        >
          retry
        </button>
      )}
    </div>
  );
}

/// Extract `{ code, message }` from whatever shape react-query handed
/// us. `ApiError` (thrown by `apiFetch`) is the structured case. Plain
/// `Error` falls back to `code: "error"` + `error.message`. Anything
/// else stringifies — but `code` is always present so the UI never has
/// to render a null badge.
function describeReviewError(error: unknown): { code: string; message: string } {
  if (error instanceof ApiError) {
    return { code: error.code, message: error.message };
  }
  if (error instanceof Error) {
    return { code: "error", message: error.message };
  }
  return { code: "error", message: String(error) };
}

function PanelHeader() {
  // The route renders an `<h2>Review</h2>` outside the Card to match the
  // surrounding `Decisions` / `Equity` headers; this is a small caption
  // inside the card that tells the operator what they're looking at.
  return (
    <div className="flex items-baseline justify-between mb-3">
      <span className="text-text-3 text-[12px] font-medium">
        Analytical review by a selected agent persona
      </span>
    </div>
  );
}

function HistoryStrip({
  reviews,
  selectedId,
  onSelect,
}: {
  reviews: EvalReview[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  return (
    <div className="flex flex-wrap gap-1.5 mb-4">
      {reviews.map((r) => {
        const isSelected = r.id === selectedId;
        const label = `${r.agent_profile_id}${
          r.verdict ? ` · ${r.verdict}` : ""
        }`;
        return (
          <button
            key={r.id}
            type="button"
            onClick={() => onSelect(r.id)}
            aria-pressed={isSelected}
            className={[
              "px-2 py-0.5 rounded-sm text-[11px] border",
              isSelected
                ? "border-gold text-gold"
                : "border-border text-text-3 hover:border-gold/60 hover:text-text-2",
            ].join(" ")}
            title={new Date(r.updated_at).toLocaleString()}
          >
            {label}
          </button>
        );
      })}
    </div>
  );
}

/// "Newest non-failed" picks the freshest Completed / Queued / Running
/// row. Falls through to the newest entry (possibly Failed) when every
/// review failed, so the operator still sees something rendered with
/// the error message instead of an empty card.
function newestNonFailed(reviews: EvalReview[] | null): EvalReview | null {
  if (!reviews || reviews.length === 0) return null;
  const fresh = reviews.find((r) => r.status !== "failed");
  return fresh ?? reviews[0];
}
