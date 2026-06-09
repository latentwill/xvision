// Live transport behavior (Task B-III / spec §2.7).
//
// Owns the pause/resume/flatten/stop mutations + per-run inline-expander UI
// state, and exposes a `transportFor(run)` factory matching the `StrategyStrip`
// seam. Each mutation:
//   1. optimistically patches the cached `AgentRunSummary[]`
//      (key `agentRunKeys.list()`) so the pill flips immediately — the strip
//      derives status from that list and would otherwise lag a 10s poll;
//   2. on success reconciles from the authoritative eval `RunSummary` the
//      mutation returns (the eval row is the source of truth);
//   3. on error reverts to the pre-mutation snapshot and surfaces an inline
//      error string (NO toast infra in this app; no popups);
//   4. on settle invalidates the list so the next poll reconciles.
//
// No popups: the pause "Positions held" choice and the stop type-to-confirm
// are inline expanders rendered by `TransportControls` under the pill. This
// hook just tracks which expander is open per run + pending/error state.

import { useCallback, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { agentRunKeys } from "@/api/agent-runs";
import { cancelRun, flattenRun, pauseRun, resumeRun } from "@/api/eval";
import type { AgentRunSummary } from "@/api/types-agent-runs";
import type { RunSummary } from "@/api/types.gen";

import {
  OPTIMISTIC_PATCH,
  patchRunInList,
  reconcileFromRunSummary,
  type TransportAction,
} from "./transport-cache";

/** Per-run inline-expander state surfaced to the pill. */
export interface TransportUiState {
  /** Pause succeeded → show the "Positions held" [Flatten]/[Keep open] expander. */
  pausedExpanderOpen: boolean;
  /** A flatten request is in flight or accepted → show "flattening…". */
  flattenPending: boolean;
  /** Stop type-to-confirm expander is open. */
  stopConfirmOpen: boolean;
  /** Last error message for this run's transport (inline, not a toast). */
  error: string | null;
  /** A mutation is in flight for this run → disable buttons to block double-fire. */
  busy: boolean;
}

const EMPTY_UI: TransportUiState = {
  pausedExpanderOpen: false,
  flattenPending: false,
  stopConfirmOpen: false,
  error: null,
  busy: false,
};

/** Handlers + UI state the pill needs for one run. */
export interface RunTransport extends TransportUiState {
  onPause: () => void;
  onResume: () => void;
  /** Open the stop type-to-confirm expander. */
  onStop: () => void;
  /** User typed the confirmation and clicked stop. */
  onStopConfirm: () => void;
  /** Dismiss the stop confirm expander without stopping. */
  onStopCancel: () => void;
  /** From the paused expander: close all open positions (run stays paused). */
  onFlatten: () => void;
  /** From the paused expander: dismiss it; positions remain open. */
  onKeepOpen: () => void;
}

function errMsg(e: unknown): string {
  if (e instanceof Error && e.message) return e.message;
  return "Request failed";
}

export function useTransport(walletDisabled: boolean) {
  const queryClient = useQueryClient();
  const [ui, setUi] = useState<Record<string, TransportUiState>>({});

  const patchUi = useCallback(
    (runId: string, patch: Partial<TransportUiState>) => {
      setUi((prev) => ({
        ...prev,
        [runId]: { ...(prev[runId] ?? EMPTY_UI), ...patch },
      }));
    },
    [],
  );

  // Shared optimistic-mutation core. `runIdOf(vars)` extracts the run id so
  // onMutate can patch the right row and snapshot for rollback.
  const listKey = agentRunKeys.list();

  const mutation = useMutation<
    RunSummary,
    unknown,
    { runId: string; action: TransportAction }
  >({
    mutationFn: ({ runId, action }) => {
      switch (action) {
        case "pause":
          return pauseRun(runId);
        case "resume":
          return resumeRun(runId);
        case "flatten":
          return flattenRun(runId);
        case "stop":
          return cancelRun(runId);
      }
    },
    onMutate: async ({ runId, action }) => {
      // Cancel in-flight list refetches so the poll can't clobber our patch.
      await queryClient.cancelQueries({ queryKey: listKey });
      const snapshot = queryClient.getQueryData<AgentRunSummary[]>(listKey);
      queryClient.setQueryData<AgentRunSummary[] | undefined>(listKey, (cur) =>
        patchRunInList(cur, runId, OPTIMISTIC_PATCH[action]),
      );
      patchUi(runId, { busy: true, error: null });
      return { snapshot };
    },
    onSuccess: (authoritative) => {
      // The eval RunSummary is the authority — reconcile the cache from it.
      queryClient.setQueryData<AgentRunSummary[] | undefined>(listKey, (cur) =>
        reconcileFromRunSummary(cur, authoritative),
      );
    },
    onError: (err, { runId }, ctx) => {
      // Revert the optimistic patch to the pre-mutation snapshot.
      const snapshot = (ctx as { snapshot?: AgentRunSummary[] } | undefined)
        ?.snapshot;
      if (snapshot !== undefined) {
        queryClient.setQueryData(listKey, snapshot);
      }
      patchUi(runId, { error: errMsg(err), flattenPending: false });
    },
    onSettled: (_data, _err, { runId }) => {
      patchUi(runId, { busy: false });
      // Reconcile against the server on the next poll.
      void queryClient.invalidateQueries({ queryKey: listKey });
    },
  });

  const transportFor = useCallback(
    (run: AgentRunSummary): RunTransport => {
      const runId = run.run_id;
      const state = ui[runId] ?? EMPTY_UI;
      // Wallet gate: omit handlers so the buttons stay disabled placeholders
      // (the strip already shows "Connect wallet to act").
      if (walletDisabled) {
        return { ...state, ...NOOP_HANDLERS };
      }
      return {
        ...state,
        onPause: () => {
          if (state.busy) return;
          mutation.mutate(
            { runId, action: "pause" },
            { onSuccess: () => patchUi(runId, { pausedExpanderOpen: true }) },
          );
        },
        onResume: () => {
          if (state.busy) return;
          // Resuming clears any open paused expander + flatten-pending badge.
          mutation.mutate(
            { runId, action: "resume" },
            {
              onSuccess: () =>
                patchUi(runId, {
                  pausedExpanderOpen: false,
                  flattenPending: false,
                }),
            },
          );
        },
        onStop: () => patchUi(runId, { stopConfirmOpen: true, error: null }),
        onStopCancel: () => patchUi(runId, { stopConfirmOpen: false }),
        onStopConfirm: () => {
          if (state.busy) return;
          mutation.mutate(
            { runId, action: "stop" },
            { onSuccess: () => patchUi(runId, { stopConfirmOpen: false }) },
          );
        },
        onFlatten: () => {
          if (state.busy) return;
          // Mark pending immediately; the run stays paused (expander stays
          // open showing "flattening…").
          patchUi(runId, { flattenPending: true });
          mutation.mutate({ runId, action: "flatten" });
        },
        onKeepOpen: () => patchUi(runId, { pausedExpanderOpen: false }),
      };
    },
    [ui, walletDisabled, mutation, patchUi],
  );

  return transportFor;
}

const NOOP_HANDLERS = {
  onPause: () => {},
  onResume: () => {},
  onStop: () => {},
  onStopConfirm: () => {},
  onStopCancel: () => {},
  onFlatten: () => {},
  onKeepOpen: () => {},
};
