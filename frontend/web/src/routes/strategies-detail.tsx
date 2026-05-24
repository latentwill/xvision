import { useCallback, useRef, useState } from "react";
import { Link, Navigate, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { apiFetch, ApiError } from "@/api/client";
import { InlineEditField } from "@/features/strategies/InlineEditField";
import { CHART2_STRATEGY_ROTATION } from "@/theme/themes";

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
  color?: string | null;
};

type StrategyDetail = {
  manifest: StrategyManifest;
};

type MetadataPatch = {
  display_name?: string;
  plain_summary?: string;
  asset_universe?: string[];
  color?: string;
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

// The 8-color rotation palette extracted for swatch display.
const ROTATION_SWATCHES = CHART2_STRATEGY_ROTATION.map((e) => e.color);

/**
 * Inline color picker row. No modal/popover — displays as a
 * horizontal swatch row with an "unset" chip and a custom
 * `<input type="color">` per the no-popup rule.
 *
 * Semantics mirror the backend convention:
 * - Click a swatch → PATCH with `{ color: "#XXXXXX" }`.
 * - Click "unset" → PATCH with `{ color: "" }` (server maps "" → None).
 * - Custom color picker → PATCH on change.
 */
function ColorPickerRow({
  currentColor,
  onPatch,
}: {
  currentColor: string | null | undefined;
  onPatch: (color: string) => Promise<void>;
}) {
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const savedTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const handlePick = useCallback(
    async (color: string) => {
      setError(null);
      try {
        await onPatch(color);
        setSaved(true);
        if (savedTimer.current) clearTimeout(savedTimer.current);
        savedTimer.current = setTimeout(() => setSaved(false), 1800);
      } catch (err) {
        const msg =
          err instanceof ApiError ? err.message : "Could not save color.";
        setError(msg);
      }
    },
    [onPatch],
  );

  const active = currentColor ?? null;

  return (
    <div data-testid="color-picker-row">
      <div
        style={{ display: "flex", alignItems: "center", gap: "6px", flexWrap: "wrap" }}
        aria-label="Strategy color swatches"
      >
        {/* 8 rotation swatches */}
        {ROTATION_SWATCHES.map((hex) => {
          const isActive = active === hex;
          return (
            <button
              key={hex}
              type="button"
              data-testid={`color-swatch-${hex}`}
              aria-label={`Set color ${hex}`}
              aria-pressed={isActive}
              onClick={() => void handlePick(hex)}
              style={{
                width: 24,
                height: 24,
                borderRadius: 4,
                background: hex,
                border: isActive
                  ? "2px solid white"
                  : "2px solid transparent",
                outline: isActive ? "2px solid " + hex : "none",
                cursor: "pointer",
                padding: 0,
                position: "relative",
                flexShrink: 0,
              }}
            >
              {isActive && (
                <span
                  aria-hidden="true"
                  style={{
                    position: "absolute",
                    inset: 0,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    color: "#fff",
                    fontSize: 13,
                    fontWeight: 700,
                    lineHeight: 1,
                    textShadow: "0 0 2px rgba(0,0,0,0.6)",
                  }}
                >
                  ✓
                </span>
              )}
            </button>
          );
        })}

        {/* Unset chip */}
        <button
          type="button"
          data-testid="color-swatch-unset"
          aria-label="Clear color"
          aria-pressed={active === null}
          onClick={() => void handlePick("")}
          style={{
            width: 24,
            height: 24,
            borderRadius: 4,
            background: "transparent",
            border: active === null ? "2px solid currentColor" : "2px dashed currentColor",
            cursor: "pointer",
            padding: 0,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: 11,
            opacity: active === null ? 1 : 0.5,
            flexShrink: 0,
          }}
        >
          —
        </button>

        {/* Custom freeform picker */}
        <label
          style={{ display: "flex", alignItems: "center", gap: 4, cursor: "pointer" }}
          aria-label="Custom color"
        >
          <input
            type="color"
            data-testid="color-picker-custom"
            aria-label="Custom color"
            value={
              active && /^#[0-9a-fA-F]{6}$/.test(active) ? active : "#000000"
            }
            onChange={(e) => void handlePick(e.target.value)}
            style={{ width: 24, height: 24, padding: 0, border: "none", cursor: "pointer" }}
          />
        </label>

        {/* Feedback indicators */}
        {saved && (
          <span
            data-testid="color-saved-indicator"
            aria-live="polite"
            style={{ fontSize: 11, opacity: 0.7 }}
          >
            saved
          </span>
        )}
        {error && (
          <span
            data-testid="color-error"
            role="alert"
            style={{ fontSize: 11, color: "var(--color-danger, #FF4D4D)" }}
          >
            {error}
          </span>
        )}
      </div>
    </div>
  );
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

  const onPatchColor = useCallback(
    async (color: string) => {
      await patchMutation.mutateAsync({ color });
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
        <dt>Color</dt>
        <dd>
          <ColorPickerRow
            currentColor={m.color}
            onPatch={onPatchColor}
          />
        </dd>
      </dl>
    </main>
  );
}
