import { useCallback, useRef, useState } from "react";
import { Link, Navigate, useNavigate, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { apiFetch, ApiError } from "@/api/client";
import { Icon } from "@/components/primitives/Icon";
import { InlineEditField } from "@/features/strategies/InlineEditField";
import { StrategyReadinessPanel } from "@/components/diagnostics/StrategyReadinessPanel";
import { CHART2_STRATEGY_ROTATION } from "@/theme/themes";
import { cloneStrategy, type TunableBound } from "@/api/strategies";

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
  tunable_bounds?: TunableBound[];
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

// ─── Tunable bounds helpers ──────────────────────────────────────────────────

/**
 * Derive a human-readable label from a dot-separated tunable-bound path.
 *
 * Supported patterns (from the Pine import surface):
 *   conditions.N.*        → "Condition N+1 threshold"
 *   mechanistic.close_policies.N.pct → "Stop/target %"
 *   mechanistic.close_policies.N.bars → "Time exit bars"
 *   mechanistic.close_policies.N.usd  → "Target PnL $"
 *   conditions.N.lhs.*   → "Condition N+1 left side"
 *   <anything else>      → humanize the last meaningful segment
 *
 * The function is intentionally simple and path-driven: if the optimizer
 * surfaces new path shapes in future, add patterns here.
 */
function deriveBoundLabel(path: string): string {
  const parts = path.split(".");

  // conditions.N.rhs.numeric  →  "Condition N+1 threshold"
  // conditions.N.*            →  "Condition N+1 threshold" (generic fallback)
  if (parts[0] === "conditions" && parts.length >= 2) {
    const idx = parseInt(parts[1] ?? "", 10);
    const n = isNaN(idx) ? 1 : idx + 1;
    const sub = parts[2] ?? "";
    if (sub === "lhs") return `Condition ${n} left side`;
    // rhs.numeric, rhs.*, or no sub → threshold
    return `Condition ${n} threshold`;
  }

  // mechanistic.close_policies.N.<field>
  if (
    parts[0] === "mechanistic" &&
    parts[1] === "close_policies" &&
    parts.length >= 4
  ) {
    const field = parts[3] ?? "";
    if (field === "pct") return "Stop/target %";
    if (field === "bars") return "Time exit bars";
    if (field === "usd") return "Target PnL $";
    // fallthrough to generic humanizer
  }

  // Generic: take the last non-numeric, non-empty segment and humanize it.
  const meaningful = parts
    .filter((p) => p.length > 0 && isNaN(parseInt(p, 10)))
    .pop();
  if (!meaningful) return path;
  // snake_case → Title Case words
  return meaningful
    .split("_")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

/**
 * Auto-generated settings strip sourced from `strategy.tunable_bounds`.
 *
 * Layout rules (CLAUDE.md, mandatory):
 * - Inline, single full-width column, no right-side box.
 * - No popups / modals / overlays.
 * - Dark-mode-safe borders: border-border only; never border-white / border-gray-*.
 * - Colored badges use low-opacity bg + dark: variants.
 *
 * v1 is READ-ONLY: it displays the declared search space.
 */
function TunableBoundsPanel({ bounds }: { bounds: TunableBound[] }) {
  if (bounds.length === 0) return null;

  return (
    <section
      aria-label="Tunable input bounds"
      data-testid="tunable-bounds-panel"
      className="mt-6"
    >
      <h2 className="text-[15px] font-medium mb-3">Input search space</h2>
      <div className="rounded border border-border bg-surface-elev overflow-hidden">
        {bounds.map((b, i) => (
          <div
            key={b.path}
            data-testid="tunable-bound-row"
            className={[
              "flex flex-wrap items-center gap-x-4 gap-y-1 px-4 py-2.5 text-[12.5px]",
              i < bounds.length - 1 ? "border-b border-border" : "",
            ]
              .filter(Boolean)
              .join(" ")}
          >
            {/* Human label */}
            <span className="font-medium text-text min-w-[140px]">
              {deriveBoundLabel(b.path)}
            </span>

            {/* Kind badge */}
            <span
              className="inline-flex items-center rounded px-1.5 py-0.5 text-[11px] font-mono
                         bg-indigo-500/10 text-indigo-700 dark:text-indigo-300"
            >
              {b.kind}
            </span>

            {/* Range */}
            <span className="text-text-2 font-mono tabular-nums">
              {b.min !== null ? b.min : "−∞"}
              {" – "}
              {b.max !== null ? b.max : "+∞"}
              {b.step !== null ? (
                <span className="text-text-3 ml-1.5">step {b.step}</span>
              ) : null}
            </span>

            {/* Path — muted mono, truncated on narrow viewports */}
            <span
              className="ml-auto text-text-3 font-mono text-[11px] truncate max-w-[260px]"
              title={b.path}
            >
              {b.path}
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}

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
                  ? "2px solid var(--border-soft)"
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
  const navigate = useNavigate();
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
  const [cloneError, setCloneError] = useState<string | null>(null);

  // Clone the strategy via the shared `POST /api/strategy/:id/clone`
  // endpoint (same flow the strategies list "Duplicate" action uses),
  // then navigate to the new draft's detail page. The display name is
  // derived from the loaded strategy at click time, so the mutation
  // closure reads `query.data` rather than the post-load `m` binding
  // (which is declared below the rules-of-hooks early returns).
  const cloneMutation = useMutation({
    mutationFn: () => {
      const name = query.data?.manifest.display_name ?? "Strategy";
      return cloneStrategy(id, { display_name: `${name} (clone)` });
    },
    onSuccess: (created) => {
      navigate(`/strategies/${encodeURIComponent(created.manifest.id)}`);
    },
    onError: (err) => {
      setCloneError(
        err instanceof ApiError ? err.message : "Could not clone strategy.",
      );
    },
  });

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
  const tunableBounds = strategy.tunable_bounds ?? [];

  return (
    <main data-testid="strategy-detail-view" data-strategy-id={m.id}>
      <header>
        <div className="flex items-center justify-between gap-3 mb-3">
          <Link
            to="/strategies"
            data-testid="strategy-detail-back"
            className="inline-flex items-center gap-1.5 text-[12px] text-text-2 hover:text-text"
          >
            ← Back to strategies
          </Link>
          <div className="flex items-center gap-2">
            {cloneError ? (
              <span
                role="alert"
                data-testid="strategy-detail-clone-error"
                title={cloneError}
                className="max-w-[240px] truncate text-[12px] text-rose-300"
              >
                {cloneError}
              </span>
            ) : null}
            <button
              type="button"
              data-testid="strategy-detail-clone"
              onClick={() => {
                setCloneError(null);
                cloneMutation.mutate();
              }}
              disabled={cloneMutation.isPending}
              className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text transition-colors hover:border-text-3 active:border-text-2 focus:outline-none focus-visible:ring-1 focus-visible:ring-text-2 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <Icon name="copy" size={13} />
              {cloneMutation.isPending ? "Cloning…" : "Clone strategy"}
            </button>
          </div>
        </div>
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
        <dt>Assets</dt>
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

      <section
        aria-label="Agent readiness"
        className="mt-6"
        data-testid="strategy-readiness-section"
      >
        <h2 className="text-[15px] font-medium mb-3">Agent readiness</h2>
        <StrategyReadinessPanel strategyId={m.id} />
      </section>

      <TunableBoundsPanel bounds={tunableBounds} />
    </main>
  );
}
