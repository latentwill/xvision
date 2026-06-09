// Live Trading cockpit shell (spec §2.3–§2.6).
//
// Layout (top → bottom, full width, single center column — NO right-side
// box; the chat rail owns the right edge per the repo layout rule):
//
//   [ strategy strip ]            ← fixed, always visible (sticky)
//   [ wallet banner ]             ← conditional (wallet not connected)
//   [ viewport ]                  ← LiveChartV2Container for the selected run
//   [ B-II slot ]                 ← account stat strip + positions table
//
// Rendered by BOTH `/live` (no preselection → auto-select most recently
// started live run) and `/live/:id` (preselect `:id`). The old
// `live-list.tsx` run list is absorbed here — its `listAgentRuns` polling
// is reused to populate the strip.
//
// B-I builds the shell + strip + wallet banner only. The stat strip,
// positions table (B-II), and transport-control behavior (B-III) are
// explicit seams below.

import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

import { agentRunKeys, listAgentRuns } from "@/api/agent-runs";
import { evalKeys, getRun } from "@/api/eval";
import { useRunStream } from "@/components/chart/use-run-stream";
import { LiveChartV2Container } from "@/components/chart/v2/surfaces/LiveChartV2Container";
import { Topbar } from "@/components/shell/Topbar";
import { useWallet } from "@/features/marketplace/lib/wallet";
import { useTraceDock } from "@/stores/trace-dock";

import { LiveAccountStrip } from "./LiveAccountStrip";
import { LivePositionsTable } from "./LivePositionsTable";
import { StrategyStrip } from "./StrategyStrip";
import { WalletBanner } from "./WalletBanner";
import { loadStripMetric, saveStripMetric, type StripMetricId } from "./strip-metrics";
import { pickDefaultRun } from "./strip-status";

export interface LiveCockpitProps {
  /** From `/live/:id`. When set, this run is preselected. */
  runId?: string;
}

export function LiveCockpit({ runId }: LiveCockpitProps) {
  const navigate = useNavigate();
  const { address } = useWallet();
  const walletDisabled = address === null;

  const runsQuery = useQuery({
    queryKey: agentRunKeys.list(),
    queryFn: () => listAgentRuns(),
    refetchInterval: 10_000,
  });
  const runs = useMemo(() => runsQuery.data ?? [], [runsQuery.data]);

  // Selection: explicit `:id` wins; otherwise auto-select the most recently
  // started live run. `userPicked` lets a click on `/live` override the
  // auto-selection without being clobbered by the 10 s poll.
  const [userPicked, setUserPicked] = useState<string | null>(null);

  const selectedId = useMemo(() => {
    if (runId) return runId;
    if (userPicked && runs.some((r) => r.run_id === userPicked)) {
      return userPicked;
    }
    return pickDefaultRun(runs)?.run_id ?? null;
  }, [runId, userPicked, runs]);

  // Configurable strip metric, persisted to localStorage.
  const [metric, setMetric] = useState<StripMetricId>(() => loadStripMetric());
  const onMetricChange = (m: StripMetricId) => {
    setMetric(m);
    saveStripMetric(m);
  };

  // Keep the trace dock pointed at the selected run (parity with the old
  // /live/:id route).
  useEffect(() => {
    useTraceDock
      .getState()
      .setActiveRun(selectedId || null, selectedId ? "live" : "post-hoc");
  }, [selectedId]);

  // ONE SSE per selected run, lifted here and shared by (a) the pill's
  // connection dot, (b) the account stat strip + positions table, and (c) the
  // chart container (via the `stream` prop, so it opens no EventSource of its
  // own). Other pills get a lightweight derived status (see StrategyStrip) so
  // we don't open an EventSource per pill. This collapses B-I's duplicate
  // stream (cockpit dot + chart each opened their own for the same run).
  const stream = useRunStream(selectedId ?? "");
  const selectedConnStatus = stream.status;

  // Decision rows for the selected run drive the open-position derivations
  // (unrealized PnL + the positions table). Reuses the eval run-detail fetch
  // path (`getRun` → RunDetail.decisions, keyed by `evalKeys.run`) so we share
  // its cache with the run-detail page — no new endpoint. Decisions are
  // fetched (not streamed); poll while the run is live.
  const decisionsQuery = useQuery({
    queryKey: evalKeys.run(selectedId ?? ""),
    queryFn: () => getRun(selectedId!),
    enabled: !!selectedId,
    refetchInterval: 10_000,
  });
  const decisions = useMemo(
    () => decisionsQuery.data?.decisions ?? [],
    [decisionsQuery.data],
  );

  const onSelect = (id: string) => {
    setUserPicked(id);
    // Only deep-linked `/live/:id` mounts keep the URL in sync with the
    // selection (re-navigating to the new id). On the bare `/live` route the
    // selection is tracked in component state (`userPicked`) and the URL is
    // intentionally left untouched.
    if (runId) navigate(`/live/${id}`);
  };

  return (
    <>
      <Topbar
        title="Live cockpit"
        sub={selectedId ?? "Real money · active deployments"}
      />

      {/* §2.4 Strategy strip — fixed, always visible. */}
      <StrategyStrip
        runs={runs}
        selectedId={selectedId}
        onSelect={onSelect}
        metric={metric}
        onMetricChange={onMetricChange}
        selectedConnStatus={selectedConnStatus}
        walletDisabled={walletDisabled}
        // B-III SEAM: supply `transportFor={(run) => ({ onPause, onResume,
        // onStop })}` to wire pause/resume/stop. Omitted here ⇒ pills render
        // the transport buttons as disabled placeholders.
      />

      {/* §2.5 Wallet banner — only when wallet not connected. Never hides data. */}
      {walletDisabled && <WalletBanner />}

      {/* §2.6 Viewport */}
      {selectedId ? (
        <section data-testid="live-viewport" className="space-y-5">
          {/* Chart consumes the lifted stream (no second EventSource). */}
          <LiveChartV2Container runId={selectedId} stream={stream} />

          {/*
            B-II — account stat strip + active positions table, full-width
            inline (NO right-side box). Both consume the lifted stream + the
            shared decisions fetch.
          */}
          <section
            data-testid="live-stats-positions-slot"
            className="space-y-5"
          >
            <LiveAccountStrip data={stream.data} decisions={decisions} />
            <LivePositionsTable data={stream.data} decisions={decisions} />
          </section>
        </section>
      ) : (
        <div className="flex flex-col items-center gap-3 py-16 text-center">
          <p className="text-[15px] font-medium text-text-2">
            No active live deployments
          </p>
          <p className="text-[13px] text-text-3">
            Configure a broker and deploy a strategy to start live trading.
          </p>
        </div>
      )}
    </>
  );
}
