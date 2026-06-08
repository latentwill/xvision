import { Link } from "react-router-dom";
import { useFlywheel } from "../api";

/**
 * FlywheelStrip — compact horizontal strip showing DSPy flywheel progress.
 *
 * Line 1: observations toward next prompt compile (cohort_count / threshold · N patterns)
 * Line 2 (only when last_prompt_compile is set): last compile gate result with link
 *
 * Hidden when the flywheel is disabled (enabled: false) or data is not yet loaded.
 *
 * No-popup rule: this is an inline strip, never a card/modal.
 */
export function FlywheelStrip() {
  const { data } = useFlywheel();

  if (!data || !data.enabled) return null;

  const {
    cohort_count,
    threshold,
    compiled_pattern_count,
    latest_optimization_run_id,
    last_prompt_compile: lpc,
  } = data;

  return (
    <div className="flex flex-col gap-0.5 px-4 py-2 rounded-md bg-surface-card border border-border/60 text-[12px] font-mono text-text-2">
      {/* Line 1: cohort progress */}
      <span>
        Observations toward next prompt compile:{" "}
        <span className="text-text font-semibold">
          {cohort_count ?? 0}/{threshold ?? "?"}
        </span>
        {" · "}
        <span className="text-text">
          {compiled_pattern_count ?? 0} patterns compiled
        </span>
      </span>

      {/* Line 2: last prompt compile result — only when present */}
      {lpc != null && (
        <span>
          Last prompt compile:{" "}
          {lpc.delta_dev != null && (
            <>
              dev{" "}
              <span className={lpc.delta_dev >= 0 ? "text-emerald-400" : "text-red-400"}>
                {lpc.delta_dev >= 0 ? "+" : ""}
                {lpc.delta_dev.toFixed(2)}
              </span>
              {" · "}
            </>
          )}
          {lpc.delta_holdout != null && (
            <>
              untouched{" "}
              <span className={lpc.delta_holdout >= 0 ? "text-emerald-400" : "text-red-400"}>
                {lpc.delta_holdout >= 0 ? "+" : ""}
                {lpc.delta_holdout.toFixed(2)}
              </span>
              {" · "}
            </>
          )}
          {lpc.gate_verdict && (
            <span
              className={
                lpc.gate_verdict.toLowerCase().startsWith("accept")
                  ? "text-emerald-400"
                  : "text-text-3"
              }
            >
              {lpc.gate_verdict}
            </span>
          )}
          {latest_optimization_run_id && (
            <>
              {" "}
              <Link
                to={`/optimizations/${latest_optimization_run_id}`}
                className="underline text-text-2 hover:text-text transition-colors"
              >
                {latest_optimization_run_id}
              </Link>
            </>
          )}
        </span>
      )}
    </div>
  );
}
