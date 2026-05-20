import { useCallback, useState } from "react";
import { Link, Navigate, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { apiFetch, ApiError } from "@/api/client";
import { InlineEditField } from "@/features/strategies/InlineEditField";

/**
 * Minimal strategy-detail route added by the
 * `strategy-edit-top-level-fields` track. The contract scopes this
 * file to the inline-edit affordance for `display_name` and
 * `plain_summary`; the broader Inspector experience continues to
 * live at `/authoring/:id`.
 *
 * Why a separate file and not an extension of `authoring.tsx`?
 * The track's `allowed_paths` list scopes frontend work to this
 * file plus `features/strategies/InlineEditField.tsx`. Reaching
 * into the Inspector would require editing `authoring.tsx`, which
 * is out of scope per the contract.
 *
 * The PATCH call is wired through `apiFetch` directly so we don't
 * need to extend `api/strategies.ts` (also out of scope).
 */

type StrategyManifest = {
  id: string;
  display_name: string;
  plain_summary: string;
  creator: string;
  template: string;
  asset_universe: string[];
  decision_cadence_minutes: number;
};

type StrategyDetail = {
  manifest: StrategyManifest;
};

type MetadataPatch = {
  display_name?: string;
  plain_summary?: string;
  asset_universe?: string[];
};

const detailQueryKey = (id: string) => ["strategy-detail", id] as const;

function getStrategy(id: string): Promise<StrategyDetail> {
  return apiFetch<StrategyDetail>(`/api/strategy/${encodeURIComponent(id)}`);
}

function patchStrategyMetadata(
  id: string,
  patch: MetadataPatch,
): Promise<StrategyDetail> {
  return apiFetch<StrategyDetail>(`/api/strategy/${encodeURIComponent(id)}`, {
    method: "PATCH",
    body: JSON.stringify(patch),
  });
}

export function StrategyDetailRoute() {
  const params = useParams<{ id?: string }>();
  if (!params.id) {
    return <Navigate to="/strategies" replace />;
  }
  return <StrategyDetailView id={params.id} />;
}

function StrategyDetailView({ id }: { id: string }) {
  const qc = useQueryClient();
  const query = useQuery({
    queryKey: detailQueryKey(id),
    queryFn: () => getStrategy(id),
  });

  // Per-field error state. We keep two separate slots so a save
  // failure on display_name doesn't blow away an existing
  // plain_summary error and vice versa.
  const [displayNameError, setDisplayNameError] = useState<string | null>(null);
  const [plainSummaryError, setPlainSummaryError] = useState<string | null>(
    null,
  );

  const patchMutation = useMutation({
    mutationFn: (patch: MetadataPatch) => patchStrategyMetadata(id, patch),
    onSuccess: (updated) => {
      // Critical: the route param `id` does not change, so the
      // detail view stays mounted. We invalidate the existing
      // query so the next render picks up the new value, but the
      // strategy id is preserved end-to-end.
      qc.setQueryData(detailQueryKey(id), updated);
    },
  });

  const onSaveDisplayName = useCallback(
    async (next: string) => {
      setDisplayNameError(null);
      try {
        await patchMutation.mutateAsync({ display_name: next });
      } catch (err) {
        const message =
          err instanceof ApiError ? err.message : "Could not save title.";
        setDisplayNameError(message);
        throw err;
      }
    },
    [patchMutation],
  );

  const onSavePlainSummary = useCallback(
    async (next: string) => {
      setPlainSummaryError(null);
      try {
        await patchMutation.mutateAsync({ plain_summary: next });
      } catch (err) {
        const message =
          err instanceof ApiError ? err.message : "Could not save description.";
        setPlainSummaryError(message);
        throw err;
      }
    },
    [patchMutation],
  );

  if (query.isPending) {
    return (
      <main data-testid="strategy-detail-view" aria-busy="true">
        Loading strategy…
      </main>
    );
  }
  if (query.isError) {
    return (
      <main data-testid="strategy-detail-view">
        <div role="alert">Could not load strategy.</div>
      </main>
    );
  }

  const strategy = query.data;
  const m = strategy.manifest;

  return (
    <main data-testid="strategy-detail-view" data-strategy-id={m.id}>
      <header>
        <Link
          to="/strategies"
          data-testid="strategy-detail-back"
          className="inline-flex items-center gap-1.5 text-[12px] text-text-2 hover:text-text mb-3"
        >
          ← Back to strategies
        </Link>
        <h1>
          <InlineEditField
            id="display-name"
            label="Strategy title"
            value={m.display_name}
            placeholder="(untitled strategy)"
            onSave={onSaveDisplayName}
            errorMessage={displayNameError}
            onEditStart={() => setDisplayNameError(null)}
          />
        </h1>
        <div
          data-testid="strategy-detail-id"
          className="font-mono text-[12px] text-text-3 break-all select-all"
          aria-label={`Strategy id ${m.id}`}
        >
          {m.id}
        </div>
        <p>
          <InlineEditField
            id="plain-summary"
            label="Strategy description"
            value={m.plain_summary}
            multiline
            placeholder="(no description)"
            onSave={onSavePlainSummary}
            errorMessage={plainSummaryError}
            onEditStart={() => setPlainSummaryError(null)}
          />
        </p>
      </header>
      <dl>
        <dt>Template</dt>
        <dd className="font-mono">{m.template}</dd>
        <dt>Creator</dt>
        <dd className="font-mono">{m.creator}</dd>
        <dt>Asset universe</dt>
        <dd>
          {m.asset_universe.length > 0
            ? m.asset_universe.join(", ")
            : "(none)"}
        </dd>
        <dt>Cadence</dt>
        <dd>every {m.decision_cadence_minutes} min</dd>
      </dl>
    </main>
  );
}
