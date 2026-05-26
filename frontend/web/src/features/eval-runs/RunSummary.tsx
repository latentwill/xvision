/**
 * RunSummary — eval-run failure-reason panel.
 *
 * Extracted from `routes/eval-runs-detail.tsx` (the inline error block
 * that has lived there since the eval inspector landed) so the
 * `eval-broker-error-circuit-breaker` track can add a classified-reason
 * banner for repeated-broker-error aborts without breaking the
 * existing rendering for un-classified failures.
 *
 * The component takes the raw persisted error string (which is the
 * `[<class>] <body>` shape produced by
 * `xvision_engine::eval::executor::format_failure_reason`). It detects
 * the `[repeated_broker_error]` prefix and renders a human-readable
 * one-liner above the raw error text. All other class prefixes — and
 * the no-prefix legacy path — render the same red-bordered code-block
 * the original UI did, so existing `Failed` runs keep their current
 * look.
 */
import type { JSX } from "react";

const REPEATED_BROKER_ERROR_PREFIX = "[repeated_broker_error]";
const BUDGET_EXCEEDED_PREFIX = "[budget_exceeded]";

/**
 * QA30 — recognise the `[budget_exceeded]` tag produced by the
 * `recovery::classify` matcher for the Cline sidecar's
 * `budget_wall_ms_exceeded` / `budget_input_tokens_exceeded` /
 * `budget_output_tokens_exceeded` step aborts. Surfaces a friendly
 * "Stopped — wall-time budget exceeded" banner above the raw text so
 * the operator doesn't see the run as an outright failure when the
 * harness simply pulled the plug on a still-responding model.
 */
export function parseBudgetExceeded(error: string): {
  kind: "wall_ms" | "input_tokens" | "output_tokens" | null;
} | null {
  if (!error.startsWith(BUDGET_EXCEEDED_PREFIX)) return null;
  if (error.includes("budget_wall_ms_exceeded")) return { kind: "wall_ms" };
  if (error.includes("budget_input_tokens_exceeded"))
    return { kind: "input_tokens" };
  if (error.includes("budget_output_tokens_exceeded"))
    return { kind: "output_tokens" };
  return { kind: null };
}

export function budgetExceededHeadline(
  kind: "wall_ms" | "input_tokens" | "output_tokens" | null,
): string {
  switch (kind) {
    case "wall_ms":
      return "Strategy stopped — wall-time budget exceeded";
    case "input_tokens":
      return "Strategy stopped — input-token budget exceeded";
    case "output_tokens":
      return "Strategy stopped — output-token budget exceeded";
    default:
      return "Strategy stopped — runtime budget exceeded";
  }
}

/**
 * Parse the executor's structured `repeated_broker_error` body. The
 * body shape produced by `paper::run_inner` is:
 *
 *   `[repeated_broker_error] repeated_broker_error: aborted after N consecutive <class_tag> rejections; run_id=… decision_index=… asset=… last_error=…`
 *
 * Returns `null` when the prefix is missing or the body doesn't parse —
 * the caller falls back to the raw error rendering.
 */
export function parseRepeatedBrokerError(error: string): {
  count: number | null;
  errorClass: string | null;
} | null {
  if (!error.startsWith(REPEATED_BROKER_ERROR_PREFIX)) {
    return null;
  }
  // "aborted after N consecutive <class_tag> rejections" — capture
  // both the count and the class tag. Tolerant of additional context
  // wrappers so a future `with_context` upstream doesn't break us.
  const match = error.match(
    /aborted after (\d+) consecutive (\S+) rejections/,
  );
  if (!match) {
    return { count: null, errorClass: null };
  }
  const count = Number.parseInt(match[1], 10);
  const errorClass = match[2];
  return {
    count: Number.isFinite(count) ? count : null,
    errorClass: errorClass || null,
  };
}

/**
 * Render the classified one-liner the dashboard surfaces above the raw
 * error text. Exported so the trace dock / eval list can reuse the
 * exact same phrasing without re-implementing the format.
 */
export function repeatedBrokerErrorHeadline(
  count: number | null,
  errorClass: string | null,
): string {
  if (count !== null && errorClass) {
    return `Aborted after ${count} consecutive ${errorClass} rejections`;
  }
  return "Aborted by the eval circuit breaker (repeated broker errors)";
}

export interface RunSummaryErrorProps {
  /**
   * Persisted error string from `RunSummary.error`. `null` /
   * `undefined` / empty string ⇒ component renders nothing.
   */
  error: string | null | undefined;
}

/**
 * Failure-reason panel for an eval run. Renders nothing when there's
 * no error. Always renders the raw error text in a code block so the
 * operator can copy/paste the underlying broker / provider response.
 * When the run aborted via the circuit breaker, a `[CIRCUIT BREAKER]`
 * banner appears above the raw text with the classified one-liner.
 */
export function RunSummaryError({ error }: RunSummaryErrorProps): JSX.Element | null {
  if (!error) return null;

  const repeated = parseRepeatedBrokerError(error);
  const budget = parseBudgetExceeded(error);

  // QA30: budget-exceeded is a CLEAN stop, not a failure. Render it on
  // a warn-tinted card rather than the danger-tinted one used for true
  // failures, and lead with the friendly headline.
  if (budget) {
    return (
      <div
        className="mt-4 p-3 border border-warn/40 bg-warn/[0.06] rounded-sm"
        data-testid="run-summary-budget-exceeded"
      >
        <div className="mb-2 text-[12px] text-warn">
          <span className="font-mono uppercase tracking-wide text-[10px] mr-2">
            stopped
          </span>
          {budgetExceededHeadline(budget.kind)}
        </div>
        <div className="text-[11px] text-text-3 uppercase tracking-wide mb-1">
          detail
        </div>
        <code className="font-mono text-[12px] text-text-2 whitespace-pre-wrap break-words">
          {error}
        </code>
      </div>
    );
  }

  return (
    <div className="mt-4 p-3 border border-danger/40 bg-danger/[0.06] rounded-sm">
      {repeated ? (
        <div
          data-testid="run-summary-circuit-breaker-banner"
          className="mb-2 text-[12px] text-danger"
        >
          <span className="font-mono uppercase tracking-wide text-[10px] mr-2">
            circuit breaker
          </span>
          {repeatedBrokerErrorHeadline(repeated.count, repeated.errorClass)}
        </div>
      ) : null}
      <div className="text-[11px] text-danger uppercase tracking-wide mb-1">
        error
      </div>
      <code className="font-mono text-[12px] text-text whitespace-pre-wrap break-words">
        {error}
      </code>
    </div>
  );
}
