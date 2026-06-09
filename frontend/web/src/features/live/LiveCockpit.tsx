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
import { useRunStream } from "@/components/chart/use-run-stream";
import { LiveChartV2Container } from "@/components/chart/v2/surfaces/LiveChartV2Container";
import { Topbar } from "@/components/shell/Topbar";
import { useWallet } from "@/features/marketplace/lib/wallet";
import { useTraceDock } from "@/stores/trace-dock";

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

  // One SSE for the selected run drives its pill's connection dot. Other
  // pills get a lightweight derived status (see StrategyStrip) so we don't
  // open an EventSource per pill. The chart owns its own stream.
  const { status: selectedConnStatus } = useRunStream(selectedId ?? "");

  const onSelect = (id: string) => {
    setUserPicked(id);
    // When deep-linked at /live/:id, navigate to keep the URL in sync.
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
          <LiveChartV2Container runId={selectedId} />

          {/*
            B-II SLOT — account stat strip + active positions table go here,
            full-width inline (NO right-side box). B-II renders:
              <LiveAccountStrip runId={selectedId} />
              <LivePositionsTable runId={selectedId} />
            Keep this as a single full-width column under the chart.
          */}
          <section data-testid="live-stats-positions-slot" aria-hidden />
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
