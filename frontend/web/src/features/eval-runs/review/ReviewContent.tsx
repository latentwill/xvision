import { FindingCard } from "./FindingCard";
import { MemoryPanel, type MemoryPanelEvent } from "./MemoryPanel";
import { VerdictBadge } from "./VerdictBadge";
import type { EvalReview, ReviewFinding } from "@/api/eval-review";
import { CANONICAL_AGENT_PROFILES } from "@/api/eval-review";

function profileLabel(id: string): string {
  return CANONICAL_AGENT_PROFILES.find((p) => p.id === id)?.label ?? id;
}

function Section({
  title,
  count,
  children,
  emptyHint,
}: {
  title: string;
  count?: number;
  children: React.ReactNode;
  emptyHint?: string;
}) {
  const isEmpty =
    Array.isArray(children) ? children.length === 0 : !children;
  return (
    <section className="mt-5">
      <h3 className="font-sans font-semibold text-[16px] text-text mb-2">
        {title}
        {typeof count === "number" && (
          <span className="text-text-3 text-[12px] font-sans ml-2">
            ({count})
          </span>
        )}
      </h3>
      {isEmpty && emptyHint ? (
        <div className="text-text-3 text-[12px] font-medium">{emptyHint}</div>
      ) : (
        children
      )}
    </section>
  );
}

function ListBlock({ items, emptyHint }: { items: string[]; emptyHint: string }) {
  if (items.length === 0) {
    return <div className="text-text-3 text-[12px] font-medium">{emptyHint}</div>;
  }
  return (
    <ul className="space-y-1.5 text-[13px] text-text-2 list-disc list-inside">
      {items.map((item, i) => (
        <li key={i}>{item}</li>
      ))}
    </ul>
  );
}

/// Render a completed `EvalReview` plus its normalized findings. The
/// review's raw JSON is parsed lazily here to surface the `risks`,
/// `next_tests`, and `questions` arrays the engine persisted — those
/// fields aren't denormalized onto `EvalReview` columns. We tolerate
/// missing/malformed raw_output_json by falling through to empty lists.
export function ReviewContent({
  review,
  findings,
}: {
  review: EvalReview;
  findings: ReviewFinding[];
}) {
  const raw = parseRaw(review.raw_output_json);

  return (
    <div>
      <div className="flex flex-wrap items-center gap-2 mb-2">
        {review.verdict && <VerdictBadge verdict={review.verdict} />}
        {typeof review.score === "number" && (
          <span className="text-text-2 text-[12px]">score {review.score}</span>
        )}
        {typeof review.confidence === "number" && (
          <span className="text-text-3 text-[12px]">
            confidence {review.confidence.toFixed(2)}
          </span>
        )}
        <span className="text-text-3 text-[11px] ml-auto">
          {profileLabel(review.agent_profile_id)} ·{" "}
          {new Date(review.updated_at).toLocaleString()}
        </span>
      </div>

      {review.summary && (
        <Section title="Executive summary">
          <p className="text-text-2 text-[14px] whitespace-pre-line">
            {review.summary}
          </p>
        </Section>
      )}

      <Section
        title="Key findings"
        count={findings.length}
        emptyHint={
          review.verdict === "inconclusive"
            ? "Verdict was inconclusive — no findings were produced."
            : "No findings recorded."
        }
      >
        {findings.length > 0 ? (
          <div className="space-y-3">
            {findings.map((f) => (
              <FindingCard key={f.id} finding={f} />
            ))}
          </div>
        ) : null}
      </Section>

      <Section title="Risks" count={raw.risks.length} emptyHint="No risks listed.">
        <ListBlock items={raw.risks} emptyHint="No risks listed." />
      </Section>

      <Section
        title="Recommended next tests"
        count={raw.next_tests.length}
        emptyHint="No next-test recommendations recorded."
      >
        <ListBlock
          items={raw.next_tests}
          emptyHint="No next-test recommendations recorded."
        />
      </Section>

      <Section title="Open questions" count={raw.questions.length} emptyHint="No open questions.">
        <ListBlock items={raw.questions} emptyHint="No open questions." />
      </Section>

      <Section title="Evidence map" emptyHint="No evidence map available.">
        <EvidenceList findings={findings} />
      </Section>

      {/* V2D Memory panel — surfaces the dispatcher's `memory_recall`,
          `memory_write`, and `memory_disabled_no_embedder` events when
          they're persisted into the review's raw output. Returns null
          (renders nothing) when the events array is empty, so reviews
          generated before the V2D wave land cleanly. */}
      <MemoryPanel events={raw.memory_events} />
    </div>
  );
}

function EvidenceList({ findings }: { findings: ReviewFinding[] }) {
  // Aggregate distinct evidence references across findings so operators
  // can see the bounded vocabulary the model cited.
  const refs = new Set<string>();
  for (const f of findings) {
    const ev = f.evidence as
      | Array<{ reference?: string }>
      | undefined;
    if (!Array.isArray(ev)) continue;
    for (const e of ev) {
      if (e.reference) refs.add(e.reference);
    }
  }
  if (refs.size === 0) {
    return (
      <div className="text-text-3 text-[12px] font-medium">
        No evidence map available.
      </div>
    );
  }
  return (
    <div className="flex flex-wrap gap-1.5">
      {Array.from(refs)
        .sort()
        .map((ref) => (
          <code
            key={ref}
            className="text-[11px] px-1.5 py-0.5 rounded-sm border border-border text-text-3 font-mono"
          >
            {ref}
          </code>
        ))}
    </div>
  );
}

type RawShape = {
  risks: string[];
  next_tests: string[];
  questions: string[];
  /// V2D dispatcher-seam events lifted onto the review's raw output JSON.
  /// Backend may populate this from the per-cycle `events.jsonl` rows;
  /// frontend tolerates absence so older reviews don't crash here.
  memory_events: MemoryPanelEvent[];
};

function parseRaw(raw: string | null): RawShape {
  if (!raw)
    return { risks: [], next_tests: [], questions: [], memory_events: [] };
  try {
    const parsed = JSON.parse(raw) as Partial<RawShape>;
    return {
      risks: Array.isArray(parsed.risks) ? parsed.risks : [],
      next_tests: Array.isArray(parsed.next_tests) ? parsed.next_tests : [],
      questions: Array.isArray(parsed.questions) ? parsed.questions : [],
      memory_events: Array.isArray(parsed.memory_events)
        ? (parsed.memory_events as MemoryPanelEvent[])
        : [],
    };
  } catch {
    return { risks: [], next_tests: [], questions: [], memory_events: [] };
  }
}
