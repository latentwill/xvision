import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FormEvent,
} from "react";
import { Link } from "react-router-dom";
import {
  useNanochatCheckpoints,
  useAutoresearchRuns,
  useAutoresearchExperiments,
  useStartRun,
  useStopRun,
  type NanochatCheckpoint,
  type AutoresearchExperiment,
  type StartRunRequest,
} from "@/api/nanochat";
import { useQuery } from "@tanstack/react-query";
import { listStrategies, strategyKeys } from "@/api/strategies";
import { useAutoresearchStream } from "../hooks/useAutoresearchStream";
import { SignalSelectMenu } from "@/components/primitives/SignalMenu";

// ─── run_tag validation ────────────────────────────────────────────────────────

const RUN_TAG_RE = /^[a-z0-9][a-z0-9-]{0,31}$/;

function validateRunTag(value: string): string | null {
  if (value.length === 0) return "Run tag is required.";
  if (value.length > 32) return "Run tag must be ≤ 32 characters.";
  if (!RUN_TAG_RE.test(value))
    return "Run tag must start with a lowercase letter or digit and contain only lowercase letters, digits, and hyphens.";
  return null;
}

// ─── Promotion toast ──────────────────────────────────────────────────────────

/** Non-focus-stealing transient toast shown when a new promoted checkpoint
 *  appears. A plain `role="status"` div that auto-dismisses after 7 s. */
function PromotionToast({
  name,
  onDismiss,
}: {
  name: string;
  onDismiss: () => void;
}) {
  useEffect(() => {
    const id = window.setTimeout(onDismiss, 7_000);
    return () => window.clearTimeout(id);
  }, [onDismiss]);

  return (
    <div
      role="status"
      aria-live="polite"
      className="fixed bottom-4 right-4 z-50 max-w-sm rounded-md border border-border bg-surface-card px-3 py-2 text-[13px] text-text shadow-lg"
    >
      Checkpoint promoted:{" "}
      <span className="font-medium">{name}</span>
    </div>
  );
}

// ─── Run launcher ─────────────────────────────────────────────────────────────

function RunLauncher({
  isRunning,
  activeRunId,
  isStarting,
  startError,
  isStopping,
  onStart,
  onStop,
}: {
  isRunning: boolean;
  activeRunId: string | null;
  isStarting: boolean;
  startError: Error | null;
  isStopping: boolean;
  onStart: (req: StartRunRequest) => void;
  onStop: () => void;
}) {
  const today = new Date().toISOString().slice(0, 10).replace(/-/g, "");
  const [sourceStrategyId, setSourceStrategyId] = useState("");
  const [strategySearch, setStrategySearch] = useState("");
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const [hoverIndex, setHoverIndex] = useState(-1);
  const [labelStrategy, setLabelStrategy] = useState<
    "price_forward" | "outcome_imitation"
  >("price_forward");
  const [threshold, setThreshold] = useState("0.01");
  const [customFilter, setCustomFilter] = useState("");
  const [runTag, setRunTag] = useState(`run${today}`);
  const [tagError, setTagError] = useState<string | null>(null);

  const { data: strategies } = useQuery({
    queryKey: strategyKeys.list(),
    queryFn: listStrategies,
    staleTime: 120_000,
  });

  const selectedName = strategies?.find(
    (s) => s.agent_id === sourceStrategyId,
  )?.display_name ?? "";

  const filtered = (strategies ?? []).filter((s) => {
    if (!strategySearch) return true;
    const q = strategySearch.toLowerCase();
    return (
      s.display_name.toLowerCase().includes(q) ||
      s.agent_id.toLowerCase().includes(q)
    );
  });

  function handleRunTagChange(v: string) {
    setRunTag(v);
    setTagError(validateRunTag(v));
  }

  function handleStart(e: FormEvent) {
    e.preventDefault();
    const err = validateRunTag(runTag);
    if (err) {
      setTagError(err);
      return;
    }
    onStart({
      source_strategy_id: sourceStrategyId,
      label_strategy: labelStrategy,
      label_config:
        labelStrategy === "price_forward"
          ? { price_forward_threshold: parseFloat(threshold) || 0.01 }
          : customFilter
            ? (() => {
                try {
                  return JSON.parse(customFilter) as Record<string, unknown>;
                } catch {
                  return {};
                }
              })()
            : {},
      run_tag: runTag,
    });
  }

  const inp = "min-h-9 rounded border border-border bg-surface-elev px-2 py-1.5 text-[13px] text-text placeholder:text-text-4";

  return (
    <section className="space-y-3">
      <h2 className="m-0 text-[13px] uppercase tracking-widest text-text-4">
        Run launcher
      </h2>
      {isRunning ? (
        <div className="flex items-center gap-3">
          <span className="text-[13px] text-text-2">
            Run <span className="font-mono">{activeRunId}</span> is active.
          </span>
          <button
            type="button"
            onClick={onStop}
            disabled={isStopping}
            className="rounded border border-danger/40 px-3 py-1.5 text-[13px] text-danger hover:bg-danger/[0.06] transition-colors disabled:opacity-60"
          >
            Stop
          </button>
        </div>
      ) : (
        <form
          onSubmit={handleStart}
          className="space-y-3 rounded-md border border-border bg-surface-card p-4"
        >
          {/* Strategy dropdown with built-in search */}
          <div className="flex flex-col gap-1 relative">
            <label
              htmlFor="ar-source-strategy"
              className="text-[12px] text-text-3"
            >
              Source strategy
            </label>
            <div className="relative">
              <input
                id="ar-source-strategy"
                type="text"
                value={dropdownOpen ? strategySearch : selectedName || strategySearch}
                placeholder="Search strategies…"
                autoComplete="off"
                onFocus={() => { setDropdownOpen(true); setHoverIndex(-1); }}
                onChange={(e) => {
                  setStrategySearch(e.target.value);
                  setDropdownOpen(true);
                  setHoverIndex(-1);
                  if (e.target.value === "") setSourceStrategyId("");
                }}
                onBlur={() => setTimeout(() => setDropdownOpen(false), 150)}
                onKeyDown={(e) => {
                  if (!dropdownOpen || filtered.length === 0) return;
                  if (e.key === "ArrowDown") {
                    e.preventDefault();
                    setHoverIndex((i) => Math.min(i + 1, filtered.length - 1));
                  } else if (e.key === "ArrowUp") {
                    e.preventDefault();
                    setHoverIndex((i) => Math.max(i - 1, -1));
                  } else if (e.key === "Enter") {
                    e.preventDefault();
                    if (hoverIndex >= 0 && hoverIndex < filtered.length) {
                      const s = filtered[hoverIndex];
                      setSourceStrategyId(s.agent_id);
                      setStrategySearch("");
                      setDropdownOpen(false);
                    }
                  } else if (e.key === "Escape") {
                    setDropdownOpen(false);
                  }
                }}
                className={`${inp} w-full pr-7`}
              />
              <span className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-[10px] text-text-4">
                {dropdownOpen ? "▲" : "▼"}
              </span>
              {dropdownOpen && (
                <div className="absolute left-0 right-0 top-full z-30 mt-0.5 max-h-56 overflow-y-auto rounded border border-border bg-surface-elev shadow-lg">
                  {filtered.length === 0 ? (
                    <div className="px-3 py-2 text-[12px] text-text-4">
                      No strategies found
                    </div>
                  ) : (
                    filtered.map((s, i) => (
                      <div
                        key={s.agent_id}
                        onMouseDown={() => {
                          setSourceStrategyId(s.agent_id);
                          setStrategySearch("");
                          setDropdownOpen(false);
                        }}
                        onMouseEnter={() => setHoverIndex(i)}
                        className={`flex items-center justify-between px-3 py-1.5 cursor-pointer text-[13px] ${
                          i === hoverIndex
                            ? "bg-accent/10 text-text"
                            : s.agent_id === sourceStrategyId
                              ? "bg-gold/5 text-text"
                              : "text-text-2 hover:bg-surface-panel"
                        }`}
                      >
                        <span className="truncate flex-1">{s.display_name}</span>
                        {s.model && (
                          <span className="ml-2 shrink-0 text-[11px] text-text-4 font-mono">
                            {s.model}
                          </span>
                        )}
                      </div>
                    ))
                  )}
                </div>
              )}
            </div>
          </div>

          <div className="flex flex-col gap-1">
            <div className="text-[12px] text-text-3">
              Label strategy
            </div>
            <SignalSelectMenu
              ariaLabel="Label strategy"
              value={labelStrategy}
              options={[
                {
                  value: "price_forward",
                  label: "price_forward — price movement baseline",
                },
                {
                  value: "outcome_imitation",
                  label: "outcome_imitation — imitate profitable cycles",
                },
              ]}
              onChange={(next) =>
                setLabelStrategy(next as "price_forward" | "outcome_imitation")
              }
              className={`${inp} w-full justify-between`}
              minWidth={240}
            />
          </div>

          {labelStrategy === "price_forward" && (
            <div className="flex flex-col gap-1">
              <label
                htmlFor="ar-threshold"
                className="text-[12px] text-text-3"
              >
                Forward-return threshold
              </label>
              <input
                id="ar-threshold"
                type="number"
                step="0.001"
                min="0"
                value={threshold}
                onChange={(e) => setThreshold(e.target.value)}
                className="w-32 rounded border border-border bg-surface-elev px-2 py-1.5 text-[13px] text-text"
              />
            </div>
          )}

          {labelStrategy === "outcome_imitation" && (
            <div className="flex flex-col gap-1">
              <label
                htmlFor="ar-custom-filter"
                className="text-[12px] text-text-3"
              >
                Custom quality filter (JSON, optional)
              </label>
              <textarea
                id="ar-custom-filter"
                value={customFilter}
                onChange={(e) => setCustomFilter(e.target.value)}
                placeholder='{"pnl": {"$gt": 0}, "drawdown_pct": {"$lt": 5}}'
                rows={3}
                className="resize-y rounded border border-border bg-surface-elev px-2 py-1.5 text-[12px] font-mono text-text placeholder:text-text-4"
              />
            </div>
          )}

          <div className="flex flex-col gap-1">
            <label htmlFor="ar-run-tag" className="text-[12px] text-text-3">
              Run tag
            </label>
            <input
              id="ar-run-tag"
              type="text"
              value={runTag}
              onChange={(e) => handleRunTagChange(e.target.value)}
              aria-describedby={tagError ? "ar-run-tag-error" : undefined}
              className="w-48 rounded border border-border bg-surface-elev px-2 py-1.5 text-[13px] font-mono text-text"
            />
            {tagError && (
              <p
                id="ar-run-tag-error"
                role="alert"
                className="text-[12px] text-danger"
              >
                {tagError}
              </p>
            )}
          </div>

          <button
            type="submit"
            disabled={isStarting || tagError != null}
            className="rounded bg-accent px-4 py-1.5 text-[13px] font-medium text-on-accent hover:opacity-90 transition-opacity disabled:cursor-not-allowed disabled:opacity-60"
          >
            {isStarting ? "Starting…" : "Start"}
          </button>

          {startError && (
            <p className="text-[12px] text-danger">
              {startError.message}
            </p>
          )}
        </form>
      )}
    </section>
  );
}

// ─── Status pill ──────────────────────────────────────────────────────────────

function StatusPill({
  status,
}: {
  status: AutoresearchExperiment["status"];
}) {
  const cls: Record<AutoresearchExperiment["status"], string> = {
    keep: "bg-green-500/10 text-green-400",
    discard: "bg-text-4/10 text-text-3",
    crash: "bg-danger/10 text-danger",
  };
  return (
    <span
      className={`inline-flex rounded-full px-2 py-0.5 text-[11px] font-medium ${cls[status]}`}
    >
      {status}
    </span>
  );
}

// ─── Live feed ────────────────────────────────────────────────────────────────

function LiveFeed({
  runId,
  experiments,
}: {
  runId: string | null;
  experiments: AutoresearchExperiment[];
}) {
  const { lines, connected } = useAutoresearchStream(runId);
  const logRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight;
    }
  }, [lines]);

  const bestAcc = experiments.reduce<number | null>((best, e) => {
    if (e.val_acc == null) return best;
    return best == null || e.val_acc > best ? e.val_acc : best;
  }, null);

  return (
    <section className="space-y-3">
      <div className="flex items-center gap-2">
        <h2 className="m-0 text-[13px] uppercase tracking-widest text-text-4">
          Live feed
        </h2>
        {runId && (
          <span
            className={[
              "h-1.5 w-1.5 rounded-full",
              connected ? "bg-green-500" : "bg-text-4",
            ].join(" ")}
            title={connected ? "Connected" : "Disconnected"}
          />
        )}
      </div>

      <div
        ref={logRef}
        className="h-40 overflow-y-auto rounded-md border border-border bg-surface-card p-3 font-mono text-[11px] text-text-2"
      >
        {lines.length === 0 ? (
          <span className="text-text-4">
            {runId ? "Waiting for output…" : "No active run."}
          </span>
        ) : (
          lines.map((l) => (
            <div key={l._row_id} className="whitespace-pre-wrap leading-relaxed">
              {l.text}
            </div>
          ))
        )}
      </div>

      {experiments.length > 0 && (
        <div className="overflow-x-auto rounded-md border border-border">
          <table className="w-full border-collapse text-[12px]">
            <thead>
              <tr className="border-b border-border text-left">
                <th className="bg-surface-panel px-3 py-2 font-medium text-text-3">
                  Commit
                </th>
                <th className="bg-surface-panel px-3 py-2 text-right font-medium text-text-3">
                  val_acc
                </th>
                <th className="bg-surface-panel px-3 py-2 text-right font-medium text-text-3">
                  val_loss
                </th>
                <th className="bg-surface-panel px-3 py-2 font-medium text-text-3">
                  Status
                </th>
                <th className="bg-surface-panel px-3 py-2 font-medium text-text-3">
                  Description
                </th>
              </tr>
            </thead>
            <tbody className="font-mono">
              {experiments.map((e) => {
                const isBest = e.val_acc != null && e.val_acc === bestAcc;
                return (
                  <tr
                    key={e.experiment_id}
                    className={[
                      "border-t border-border",
                      isBest ? "bg-gold/[0.06]" : "hover:bg-surface-elev/30",
                    ].join(" ")}
                  >
                    <td className="px-3 py-1.5 text-text-3">{e.git_commit}</td>
                    <td className="px-3 py-1.5 text-right">
                      {e.val_acc == null ? (
                        <span className="text-danger">crash</span>
                      ) : (
                        <span
                          className={
                            isBest
                              ? "font-semibold text-gold"
                              : "text-text-2"
                          }
                        >
                          {e.val_acc.toFixed(3)}
                        </span>
                      )}
                    </td>
                    <td className="px-3 py-1.5 text-right text-text-2">
                      {e.val_loss?.toFixed(3) ?? "—"}
                    </td>
                    <td className="px-3 py-1.5">
                      <StatusPill status={e.status} />
                    </td>
                    <td
                      className="max-w-xs truncate px-3 py-1.5 text-text-3"
                      title={e.description}
                    >
                      {e.description}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

// ─── Checkpoint leaderboard ───────────────────────────────────────────────────

function CheckpointLeaderboard({
  checkpoints,
}: {
  checkpoints: NanochatCheckpoint[];
}) {
  const promoted = [...checkpoints]
    .filter((c) => c.promoted)
    .sort((a, b) => (b.best_acc ?? 0) - (a.best_acc ?? 0));

  return (
    <section className="space-y-3">
      <h2 className="m-0 text-[13px] uppercase tracking-widest text-text-4">
        Checkpoint leaderboard
      </h2>
      {promoted.length === 0 ? (
        <p className="text-[12px] text-text-3">No promoted checkpoints yet.</p>
      ) : (
        <div className="overflow-x-auto rounded-md border border-border">
          <table className="w-full border-collapse text-[12px]">
            <thead>
              <tr className="border-b border-border text-left">
                <th className="bg-surface-panel px-3 py-2 font-medium text-text-3">
                  Name
                </th>
                <th className="bg-surface-panel px-3 py-2 text-right font-medium text-text-3">
                  val_acc
                </th>
                <th className="bg-surface-panel px-3 py-2 text-right font-medium text-text-3">
                  Lift vs baseline
                </th>
                <th className="bg-surface-panel px-3 py-2 font-medium text-text-3">
                  Source strategy
                </th>
                <th className="bg-surface-panel px-3 py-2 font-medium text-text-3">
                  Run tag
                </th>
                <th className="bg-surface-panel px-3 py-2 font-medium text-text-3">
                  Status
                </th>
                <th className="bg-surface-panel px-3 py-2 font-medium text-text-3">
                  Action
                </th>
              </tr>
            </thead>
            <tbody className="font-mono">
              {promoted.map((c) => (
                <tr
                  key={c.model_id}
                  className="border-t border-border hover:bg-surface-elev/30"
                >
                  <td
                    className="max-w-xs truncate px-3 py-1.5 text-text"
                    title={c.display_name}
                  >
                    {c.display_name}
                  </td>
                  <td className="px-3 py-1.5 text-right text-text-2">
                    {c.best_acc?.toFixed(3) ?? "—"}
                  </td>
                  <td className="px-3 py-1.5 text-right text-text-3">
                    <span title="Run a backtest via the strategy builder to measure precision lift">
                      — run backtest to measure
                    </span>
                  </td>
                  <td className="px-3 py-1.5 text-text-3">
                    {c.source_strategy_name ?? c.source_strategy_id ?? "—"}
                  </td>
                  <td className="px-3 py-1.5 text-text-3">{c.run_tag}</td>
                  <td className="px-3 py-1.5">
                    {c.live_approved ? (
                      <span className="inline-flex rounded-full bg-green-500/10 px-2 py-0.5 text-[11px] font-medium text-green-400">
                        Approved
                      </span>
                    ) : (
                      <span className="inline-flex rounded-full bg-amber-500/10 px-2 py-0.5 text-[11px] font-medium text-amber-400">
                        Candidate
                      </span>
                    )}
                  </td>
                  <td className="px-3 py-1.5">
                    {c.source_strategy_id ? (
                      <Link
                        to={`/strategies/${encodeURIComponent(c.source_strategy_id)}?attach_checkpoint=${encodeURIComponent(c.model_id)}`}
                        className="text-[12px] text-accent underline-offset-2 hover:underline"
                      >
                        Attach to strategy
                      </Link>
                    ) : (
                      <span className="text-[12px] text-text-4">
                        No source strategy
                      </span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

// ─── Root component ───────────────────────────────────────────────────────────

export function AutoresearcherTab() {
  const runsQ = useAutoresearchRuns();
  const runs = runsQ.data ?? [];

  // Active run = the most recent one with status "running"
  const activeRun = runs.find((r) => r.status === "running") ?? null;
  const activeRunId = activeRun?.run_id ?? null;

  const experimentsQ = useAutoresearchExperiments(activeRunId);
  const checkpointsQ = useNanochatCheckpoints({ promoted_only: true });

  const checkpoints = checkpointsQ.data ?? [];
  const experiments = experimentsQ.data ?? [];

  // Track promoted checkpoint model_ids to detect new promotions for toast
  const prevPromotedRef = useRef<Set<string>>(new Set());
  const [promotionToast, setPromotionToast] = useState<{
    name: string;
  } | null>(null);

  useEffect(() => {
    const prev = prevPromotedRef.current;
    for (const c of checkpoints) {
      if (c.promoted && !prev.has(c.model_id)) {
        setPromotionToast({ name: c.display_name });
      }
    }
    prevPromotedRef.current = new Set(
      checkpoints.filter((c) => c.promoted).map((c) => c.model_id),
    );
  }, [checkpoints]);

  const startMutation = useStartRun();
  const stopMutation = useStopRun();

  const handleStart = useCallback(
    (req: StartRunRequest) => startMutation.mutate(req),
    [startMutation],
  );

  const handleStop = useCallback(() => {
    if (activeRunId) stopMutation.mutate(activeRunId);
  }, [activeRunId, stopMutation]);

  return (
    <div className="space-y-8">
      <RunLauncher
        isRunning={activeRunId != null}
        activeRunId={activeRunId}
        isStarting={startMutation.isPending}
        startError={startMutation.error instanceof Error ? startMutation.error : null}
        isStopping={stopMutation.isPending}
        onStart={handleStart}
        onStop={handleStop}
      />

      <LiveFeed runId={activeRunId} experiments={experiments} />

      <CheckpointLeaderboard checkpoints={checkpoints} />

      {promotionToast && (
        <PromotionToast
          name={promotionToast.name}
          onDismiss={() => setPromotionToast(null)}
        />
      )}
    </div>
  );
}
