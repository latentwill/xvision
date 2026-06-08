import { useEffect, useState } from "react";

/** The 7 canonical optimizer phases shown as a horizontal stepper. */
export const OPTIMIZER_PHASES = [
  "Briefing",
  "Parent selection",
  "Writing experiment",
  "Evaluating",
  "Gate review",
  "Committing",
  "Finishing",
] as const;

export type OptimizerPhase = (typeof OPTIMIZER_PHASES)[number];

interface PhaseStepperProps {
  /** The phase that is currently active (highlighted + elapsed ticker). Null = all neutral. */
  currentPhase: string | null;
  /** Phases that have already completed — rendered dimmed with ✓. */
  completedPhases: string[];
}

/**
 * Horizontal row of 7 phase chips for the Optimizer status hero.
 *
 * Layout rule: inline full-width strip (no right-side box, per the dashboard
 * three-pane rule). Renders as a horizontally-scrollable row on narrow viewports.
 */
export function PhaseStepper({ currentPhase, completedPhases }: PhaseStepperProps) {
  // Tick elapsed seconds for the current phase
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    if (!currentPhase) {
      setElapsed(0);
      return;
    }
    setElapsed(0);
    const timer = setInterval(() => setElapsed((s) => s + 1), 1_000);
    return () => clearInterval(timer);
  }, [currentPhase]);

  return (
    <div
      role="list"
      aria-label="Optimizer phases"
      className="flex items-center gap-1.5 overflow-x-auto py-1 scrollbar-none"
    >
      {OPTIMIZER_PHASES.map((phase) => {
        const isCurrent = phase === currentPhase;
        const isDone = completedPhases.includes(phase);

        return (
          <div
            key={phase}
            role="listitem"
            data-current={isCurrent || undefined}
            data-completed={isDone || undefined}
            className={[
              "flex-shrink-0 flex items-center gap-1 px-2.5 py-1 rounded-sm border text-[11px] whitespace-nowrap transition-colors",
              isCurrent
                ? "border-gold/50 bg-gold/10 text-gold font-medium"
                : isDone
                  ? "border-border text-text-3 opacity-50"
                  : "border-border text-text-3",
            ].join(" ")}
          >
            {isDone && <span aria-hidden>✓</span>}
            <span>{phase}</span>
            {isCurrent && elapsed > 0 && (
              <span className="font-mono text-[10px] text-gold/70 ml-0.5">{elapsed}s</span>
            )}
          </div>
        );
      })}
    </div>
  );
}
