// Live Trading shell (spec §2.3–§2.6).
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
import { listStrategies, strategyKeys } from "@/api/strategies";
import { useRunStream } from "@/components/chart/use-run-stream";
import { LiveChartV2Container } from "@/components/chart/v2/surfaces/LiveChartV2Container";
import { Topbar } from "@/components/shell/Topbar";
import { useWallet } from "@/features/marketplace/lib/wallet";
import { displayStrategyName } from "@/lib/run-display";
import { useTraceDock } from "@/stores/trace-dock";

import { ArenaStandingIndicator } from "./ArenaStandingIndicator";
import { DegenDeployStrip, type DegenDeployPayload } from "./DegenDeployStrip";
import { LiveAccountStrip } from "./LiveAccountStrip";
import { LivePositionsTable } from "./LivePositionsTable";
import { VenueAccountPanel } from "./VenueAccountPanel";
import { StrategyStrip } from "./StrategyStrip";
import { useDeployDegenArena } from "./useDeployDegenArena";
import { WalletBanner } from "./WalletBanner";
import { dailyPnl } from "./live-account";
import { pickDefaultRun } from "./strip-status";
import { useTransport } from "./useTransport";

export interface LiveConsoleProps {
  /** From `/live/:id`. When set, this run is preselected. */
  runId?: string;
}

export function LiveConsole({ runId }: LiveConsoleProps) {
  const navigate = useNavigate();
  const { address } = useWallet();
  const walletDisabled = address === null;

  // B-III transport: pause/resume/flatten/stop with optimistic cache updates
  // against `agentRunKeys.list()`. The factory hands each pill its run's
  // handlers + inline-expander UI state.
  const transportFor = useTransport(walletDisabled);

  // Degen Arena deploy: persist the operator's trade-only HL key via the deploy
  // route (POST /api/live/deploy/degen-arena); then they map a strategy to a
  // live run. Inline status only — no popup (repo UI rule).
  const deployDegenArena = useDeployDegenArena();
  const [degenDeployMsg, setDegenDeployMsg] = useState<string | null>(null);
  const handleDegenDeploy = (payload: DegenDeployPayload) => {
    setDegenDeployMsg("Deploying…");
    void deployDegenArena(payload)
      .then(() =>
        setDegenDeployMsg(
          "Stored — Degen Arena key saved. Map a strategy and start a live run.",
        ),
      )
      .catch((e: unknown) =>
        setDegenDeployMsg(
          e instanceof Error ? `Deploy failed: ${e.message}` : "Deploy failed",
        ),
      );
  };

  const runsQuery = useQuery({
    queryKey: agentRunKeys.list(),
    queryFn: () => listAgentRuns(),
    refetchInterval: 10_000,
  });
  const runs = useMemo(() => runsQuery.data ?? [], [runsQuery.data]);

  const strategiesQuery = useQuery({
    queryKey: strategyKeys.list(),
    queryFn: listStrategies,
  });
  const strategies = strategiesQuery.data ?? [];

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
  // stream (connection dot + chart each opened their own for the same run).
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

  const selectedRun = selectedId
    ? runs.find((r) => r.run_id === selectedId)
    : undefined;
  const topbarSub = selectedRun
    ? displayStrategyName(selectedRun.agent_id ?? "", strategies)
    : "Live trading · active deployments";

  return (
    <>
      <Topbar
        title="Live Trading"
        sub={topbarSub}
      />

      {/* §2.4 Strategy strip — fixed, always visible. */}
      <StrategyStrip
        runs={runs}
        selectedId={selectedId}
        onSelect={onSelect}
        selectedConnStatus={selectedConnStatus}
        walletDisabled={walletDisabled}
        transportFor={transportFor}
        strategies={strategies}
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

          {/*
            Arena standing — full-width inline chip row (no right-side box,
            no popup). Props are derived conservatively:
              - tradingViaArena: false until the run's execution venue is
                exposed in AgentRunSummary. TODO(degen standing): derive
                from run venue once exposed (look for a 'degen_arena' /
                broker_creds_ref value in the run's live_config).
              - aiPotInView: same gating as tradingViaArena for now.
              - pnlUsd: daily PnL from the lifted equity stream (same
                derivation as LiveAccountStrip).
              - rank: null — no arena-standings API yet.
          */}
          <ArenaStandingIndicator
            tradingViaArena={false /* TODO(degen standing): derive from run venue */}
            aiPotInView={false /* TODO(degen standing): derive from run venue */}
            rank={null}
            pnlUsd={
              stream.data
                ? (dailyPnl(stream.data.equity).usd ?? null)
                : null
            }
          />
        </section>
      ) : (
        <div className="space-y-5 py-8">
          <div className="flex flex-col items-center gap-3 text-center">
            <p className="text-[15px] font-medium text-text-2">
              No active live deployments
            </p>
            <p className="text-[13px] text-text-3">
              Configure a broker and deploy a strategy to start live trading.
            </p>
          </div>
          {/* Degen Arena (Hyperliquid via Virtuals) deploy strip — inline,
              full-width per the layout rule. */}
          <DegenDeployStrip onDeploy={handleDegenDeploy} />
          {degenDeployMsg && (
            <p
              data-testid="degen-deploy-status"
              className="text-center text-[12px] text-text-3"
            >
              {degenDeployMsg}
            </p>
          )}
        </div>
      )}

      {/*
        Venue account — the execution venue's OWN ledger (Orderly), independent
        of run selection: real equity / USDC / positions behind live runs.
        Full-width inline band per the layout rule.
      */}
      <div className="mt-5">
        <VenueAccountPanel />
      </div>
    </>
  );
}
