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
        <div className="text-text-3 text-[13px] italic">
          Reviews are available after the run finishes.
        </div>
      </Card>
    );
  }

  const reviews = listQuery.data ?? [];
  const hasReviews = reviews.length > 0;
  const detail = detailQuery.data ?? null;
  const detailReview: EvalReview | null = detail?.review ?? null;

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
          <div className="text-danger text-[12px] mt-2">
            {generate.error instanceof Error
              ? generate.error.message
              : String(generate.error)}
          </div>
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
        <div className="text-text-3 text-[13px] italic">
          Loading reviews…
        </div>
      )}

      {listQuery.isSuccess && !hasReviews && !generate.isPending && (
        <div className="text-text-2 text-[13px]">
          No review yet for this run. Pick an agent above to generate one.
        </div>
      )}

      {generate.isPending && (
        <div
          className="text-text-2 text-[13px] italic"
          role="status"
          aria-live="polite"
        >
          Generating review… this can take 10–30 seconds depending on the
          provider.
        </div>
      )}

      {detailQuery.isPending && effectiveId && (
        <div className="text-text-3 text-[13px] italic">Loading review…</div>
      )}

      {detail && detailReview && (
        <>
          {detailReview.status === "failed" && (
            <div className="border border-danger/40 rounded-card p-3 mb-3 text-danger text-[13px]">
              Review failed: {detailReview.error ?? "(no detail recorded)"}
            </div>
          )}
          {detailReview.status === "completed" && (
            <ReviewContent
              review={detailReview}
              findings={detail.findings}
            />
          )}
          {(detailReview.status === "queued" ||
            detailReview.status === "running") && (
            <div className="text-text-3 text-[13px] italic">
              Review in flight ({detailReview.status})…
            </div>
          )}
        </>
      )}
    </Card>
  );
}

function PanelHeader() {
  // The route renders an `<h2>Review</h2>` outside the Card to match the
  // surrounding `Decisions` / `Equity` headers; this is a small caption
  // inside the card that tells the operator what they're looking at.
  return (
    <div className="flex items-baseline justify-between mb-3">
      <span className="text-text-3 text-[12px] italic">
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
