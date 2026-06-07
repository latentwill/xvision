import { useState } from "react";
import { Link } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { Pill } from "@/components/primitives/Pill";
import { LiveCycleView } from "../LiveCycleView";
import { RecentCyclesTable } from "../panels/RecentCyclesTable";
import { ExperimentWritersPanel } from "../panels/ExperimentWritersPanel";
import { PhaseStepper } from "../ui/PhaseStepper";
import { FlywheelStrip } from "../ui/FlywheelStrip";
import { ImprovementChart } from "../ui/ImprovementChart";
import { OutcomeStackedChart } from "../ui/OutcomeStackedChart";
import { ModePicker, type RunMode } from "../ui/ModePicker";
import { useOptimizerStatus, useOptimizerStats, useSessionList, type SessionListItem } from "../api";

// ─── State pill helper ────────────────────────────────────────────────────────

function StatePill({ state }: { state: string }) {
  const lower = state.toLowerCase();
  if (lower === "running")
    return (
      <Pill tone="gold" animated>
        Running
      </Pill>
    );
  if (lower === "paused") return <Pill tone="warn">Paused</Pill>;
  if (lower === "cancelling") return <Pill tone="warn">Cancelling</Pill>;
  if (lower === "finished") return <Pill tone="default">Finished</Pill>;
  if (lower === "failed") return <Pill tone="danger">Failed</Pill>;
  return <Pill tone="default">Idle</Pill>;
}

function modeLabel(mode: string): string {
  if (mode === "explore") return "Explore";
  if (mode === "exploit") return "Exploit";
  return mode || "";
}

function formatRelativeTime(iso?: string): string {
  if (!iso) return "";
  try {
    const diffMs = Date.now() - new Date(iso).getTime();
    const diffMin = Math.floor(diffMs / 60_000);
    if (diffMin < 1) return "just now";
    if (diffMin < 60) return `${diffMin}m ago`;
    const diffHr = Math.floor(diffMin / 60);
    if (diffHr < 24) return `${diffHr}h ago`;
    return `${Math.floor(diffHr / 24)}d ago`;
  } catch {
    return iso;
  }
}

// ─── Status hero ──────────────────────────────────────────────────────────────

function StatusHero() {
  const status = useOptimizerStatus();
  const session = status?.active_session ?? null;
  const state = session?.state ?? "idle";
  const isRunning = state === "running";
  const isPaused = state === "paused";
  const isCancelling = state === "cancelling";
  const isActive = isRunning || isPaused || isCancelling;

  return (
    <div className="rounded-md border border-border bg-surface-card px-5 py-4 space-y-3">
      <div className="flex items-start justify-between gap-4 flex-wrap">
        <div className="space-y-1.5">
          <div className="flex items-center gap-2">
            <span className="uppercase tracking-[0.22em] text-[9.5px] text-text-3 font-medium">
              Optimizer
            </span>
            <StatePill state={state} />
          </div>
          {isActive && session ? (
            <h2 className="text-lg font-semibold tracking-tight text-text">
              Run {session.session_id.slice(0, 8)} · {session.strategy_id} ·{" "}
              {modeLabel(session.mode)}
            </h2>
          ) : (
            <h2 className="text-lg font-semibold tracking-tight text-text-3">No run in progress</h2>
          )}
          {isActive && session && (
            <p className="font-mono text-[11.5px] text-text-3">
              {session.cycles_completed} cycles · {session.kept_count} kept ·{" "}
              {session.suspect_count} suspect · {session.dropped_count} dropped
            </p>
          )}
        </div>
        <div className="flex items-center gap-2 flex-wrap justify-end">
          {isRunning && session && (
            <>
              <button
                type="button"
                disabled
                title="Pause"
                className="rounded border border-border px-3 py-1.5 text-[13px] text-text-2 opacity-60 cursor-not-allowed"
              >
                Pause
              </button>
              <button
                type="button"
                disabled
                title="Cancel"
                className="rounded border border-danger/40 px-3 py-1.5 text-[13px] text-danger opacity-60 cursor-not-allowed"
              >
                Cancel
              </button>
            </>
          )}
          {isPaused && session && (
            <>
              <button
                type="button"
                disabled
                title="Resume"
                className="rounded bg-accent px-3 py-1.5 text-[13px] font-medium text-on-accent opacity-60 cursor-not-allowed"
              >
                Resume
              </button>
              <button
                type="button"
                disabled
                title="Cancel"
                className="rounded border border-danger/40 px-3 py-1.5 text-[13px] text-danger opacity-60 cursor-not-allowed"
              >
                Cancel
              </button>
            </>
          )}
        </div>
      </div>
      {isRunning && (
        <PhaseStepper currentPhase={null} completedPhases={[]} />
      )}
    </div>
  );
}

// ─── Configure strip (P4) ─────────────────────────────────────────────────────

/** Run configure section — shown when no session is active.
 *  Lets the operator pick run mode (once / N experiments / until budget) then
 *  start a new session via POST /sessions. */
function ConfigureSection() {
  const status = useOptimizerStatus();
  const isActive =
    status?.active_session != null &&
    ["running", "paused", "cancelling"].includes(status.active_session.state);

  const [runMode, setRunMode] = useState<RunMode>("once");
  const [runCount, setRunCount] = useState<number | undefined>(undefined);
  const [runBudget, setRunBudget] = useState<number | undefined>(undefined);

  if (isActive) return null;

  function handleModeChange(mode: RunMode, count?: number, budget?: number) {
    setRunMode(mode);
    setRunCount(count);
    setRunBudget(budget);
  }

  function handleStart() {
    // Build a start request from the selected mode.
    // The actual POST /sessions API integration would use a strategy_id
    // from the user's strategy picker (not in scope for ModePicker alone).
    // For now, scroll to #optimizer-run-controls where the full launch form lives.
    const el = document.getElementById("optimizer-run-controls");
    if (el) el.scrollIntoView({ behavior: "smooth" });
  }

  return (
    <div
      id="optimizer-run-controls"
      className="rounded-md border border-border bg-surface-card px-5 py-4 space-y-3"
    >
      <div>
        <h2 className="text-[13px] font-semibold tracking-tight text-text">Configure run</h2>
        <p className="text-[11px] text-text-3 mt-0.5">
          Choose how many experiments to run before stopping
        </p>
      </div>
      <ModePicker value={runMode} onChange={handleModeChange} />
      <div className="flex items-center gap-3 pt-1">
        <button
          type="button"
          onClick={handleStart}
          className="rounded bg-accent px-3 py-1.5 text-[13px] font-medium text-on-accent hover:opacity-90 transition-opacity"
          data-testid="configure-start-button"
        >
          Start
        </button>
        {runMode === "n_experiments" && runCount != null && (
          <span className="text-[11px] text-text-3">
            Will run {runCount} experiment{runCount !== 1 ? "s" : ""}
          </span>
        )}
        {runMode === "until_budget" && runBudget != null && (
          <span className="text-[11px] text-text-3">
            Budget cap: ${runBudget.toFixed(2)}
          </span>
        )}
      </div>
    </div>
  );
}

// ─── Recent sessions list ─────────────────────────────────────────────────────

function SessionStateChip({ state }: { state: string }) {
  return <StatePill state={state} />;
}

function RecentSessionRow({ item }: { item: SessionListItem }) {
  return (
    <Link
      to={`/optimizer/run/${item.session_id}`}
      className="flex items-center gap-3 px-4 py-2.5 border-b border-border/50 last:border-0 hover:bg-surface-elev/40 transition-colors group"
    >
      <SessionStateChip state={item.state} />
      <span className="font-mono text-[12px] text-text truncate flex-1">{item.strategy_id}</span>
      <span className="text-[11px] text-text-3">{modeLabel(item.mode)}</span>
      {item.kept_count > 0 && (
        <span className="font-mono text-[11px] text-gold">{item.kept_count} kept</span>
      )}
      {item.finished_at && (
        <span className="font-mono text-[11px] text-text-3">
          {formatRelativeTime(item.finished_at)}
        </span>
      )}
      <span className="text-text-3 text-[11px] opacity-0 group-hover:opacity-100 transition-opacity">
        →
      </span>
    </Link>
  );
}

function RecentSessionsList() {
  const { data: sessions, isLoading } = useSessionList();

  if (isLoading) return null;
  if (!sessions || sessions.length === 0) {
    return (
      <div className="rounded-md border border-border px-4 py-3">
        <p className="text-[13px] text-text-3">No runs yet</p>
      </div>
    );
  }

  return (
    <div className="space-y-2">
      <h2 className="text-sm font-semibold text-text">Recent runs</h2>
      <div className="rounded-md border border-border overflow-hidden bg-surface-card">
        {sessions.map((item) => (
          <RecentSessionRow key={item.session_id} item={item} />
        ))}
      </div>
    </div>
  );
}

// ─── Improvement + outcome charts (P3-W3) ────────────────────────────────────

function ImprovementChartsSection() {
  const [showOutcome, setShowOutcome] = useState(false);
  const { data: statsRows } = useOptimizerStats();
  const rows = statsRows ?? [];

  return (
    <div className="rounded-md border border-border bg-surface-card px-5 py-4 space-y-3">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h2 className="text-[13px] font-semibold tracking-tight text-text">Improvement over time</h2>
          <p className="text-[11px] text-text-3 mt-0.5">Best Δ untouched-period score per cycle</p>
        </div>
        <button
          type="button"
          onClick={() => setShowOutcome((v) => !v)}
          className="rounded border border-border px-2.5 py-1 text-[11px] text-text-2 hover:bg-surface-elev/40 transition-colors"
        >
          {showOutcome ? "Hide outcome mix" : "Show outcome mix"}
        </button>
      </div>
      <ImprovementChart rows={rows} />
      {showOutcome && <OutcomeStackedChart rows={rows} />}
    </div>
  );
}

// ─── Page root ────────────────────────────────────────────────────────────────

export function OptimizerHome() {
  return (
    <>
      <Topbar title="Optimizer" sub="Tonight's run, experiment writers, and recent cycles" />
      <div className="space-y-5">
        {/* Server-driven status hero (P1) */}
        <StatusHero />

        {/* Configure strip — shown when idle (P4) */}
        <ConfigureSection />

        {/* Improvement chart + outcome mix toggle (P3-W3) */}
        <ImprovementChartsSection />

        {/* DSPy flywheel progress strip (P3-W4) — hidden when dspy_enabled=false */}
        <FlywheelStrip />

        {/* In-flight cycle + live event feed (existing dashboard body). */}
        <LiveCycleView embedded />

        <ExperimentWritersPanel />
        <RecentCyclesTable />

        {/* Recent session runs list linking to /optimizer/run/:id */}
        <RecentSessionsList />
      </div>
    </>
  );
}
