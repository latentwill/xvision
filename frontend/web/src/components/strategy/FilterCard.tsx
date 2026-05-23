// FilterCard — operator surface for the per-strategy deterministic
// firing filter. Per `docs/superpowers/specs/2026-05-22-agent-firing-filter-operator-surface.md`,
// the MVP exposes a free-form text editor that round-trips through
// `PUT /api/strategy/:id/filter` (parsed + validated server-side).
// No popups, no overlays — everything inline per the SPA UI rule in
// `/CLAUDE.md`.

import { useEffect, useMemo, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Card } from "@/components/primitives/Card";
import { ApiError } from "@/api/client";
import {
  clearStrategyFilter,
  setStrategyFilter,
  strategyKeys,
  type Strategy,
} from "@/api/strategies";

type Format = "toml" | "json";

function initialSourceFor(
  filter: Strategy["filter"],
): { source: string; format: Format } {
  if (filter == null) {
    return { source: "", format: "toml" };
  }
  // Engine canonicalizes the filter into a JSON object on read; we
  // round-trip as pretty-printed JSON until a TOML serializer ships
  // for the existing value. Operators can still author + save TOML —
  // the format radio drives what gets sent on Save.
  return { source: JSON.stringify(filter, null, 2), format: "json" };
}

export function FilterCard({ strategy }: { strategy: Strategy }) {
  const qc = useQueryClient();
  const strategyId = strategy.manifest.id;
  const initial = useMemo(
    () => initialSourceFor(strategy.filter ?? null),
    [strategy.filter],
  );
  const [source, setSource] = useState<string>(initial.source);
  const [format, setFormat] = useState<Format>(initial.format);
  const [savedFlash, setSavedFlash] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  // Reset local state when the server-side filter changes (e.g. after
  // an invalidate + refetch). Without this, the textarea would stay
  // stuck on the last-edited value after a successful save.
  useEffect(() => {
    setSource(initial.source);
    setFormat(initial.format);
    setLocalError(null);
  }, [initial.source, initial.format]);

  const hasFilter = strategy.filter != null;

  const saveMut = useMutation({
    mutationFn: () =>
      setStrategyFilter(strategyId, { source, format }),
    onSuccess: () => {
      setLocalError(null);
      setSavedFlash(true);
      window.setTimeout(() => setSavedFlash(false), 1800);
      qc.invalidateQueries({ queryKey: strategyKeys.detail(strategyId) });
      qc.invalidateQueries({ queryKey: strategyKeys.validate(strategyId) });
    },
    onError: (err) => {
      setLocalError(filterErrorMessage(err));
    },
  });

  const clearMut = useMutation({
    mutationFn: () => clearStrategyFilter(strategyId),
    onSuccess: () => {
      setLocalError(null);
      setSavedFlash(true);
      window.setTimeout(() => setSavedFlash(false), 1800);
      qc.invalidateQueries({ queryKey: strategyKeys.detail(strategyId) });
      qc.invalidateQueries({ queryKey: strategyKeys.validate(strategyId) });
    },
    onError: (err) => {
      setLocalError(filterErrorMessage(err));
    },
  });

  const busy = saveMut.isPending || clearMut.isPending;
  const canSave = source.trim().length > 0 && !busy;

  return (
    <Card>
      <header className="px-5 pt-4 pb-3 border-b border-border-soft">
        <div className="flex flex-wrap items-center gap-2">
          <div className="text-[12px] uppercase tracking-wide text-text-3">
            Filter
          </div>
          <span
            className={`rounded-sm border px-2 py-0.5 font-mono text-[11px] ${
              hasFilter
                ? "border-success/40 bg-success/[0.08] text-success"
                : "border-warn/35 bg-warn/[0.08] text-warn"
            }`}
          >
            {hasFilter ? "Filter artifact attached" : "No filter artifact attached"}
          </span>
        </div>
        <div className="text-[12px] text-text-2 mt-0.5">
          Deterministic gate that fires the strategy on selected bars only.
          Prompt wording alone is not an XVN filter artifact.
        </div>
      </header>

      <div className="px-5 pt-4 pb-5 space-y-4">
        <textarea
          spellCheck={false}
          value={source}
          onChange={(e) => {
            setSource(e.target.value);
            setLocalError(null);
          }}
          placeholder={
            format === "toml"
              ? '# Example\n# rule = "rsi(14) < 30"\n'
              : '{\n  "rule": "rsi(14) < 30"\n}'
          }
          rows={10}
          className="w-full min-h-[180px] bg-surface-elev border border-border rounded px-3 py-2 text-[12px] text-text font-mono leading-relaxed focus:outline-none focus:border-text-3"
        />

        <div className="flex flex-wrap items-center gap-x-5 gap-y-2 text-[12px] text-text-2">
          <span className="text-[11px] uppercase tracking-wide text-text-3">
            Format
          </span>
          <label className="inline-flex items-center gap-1.5 cursor-pointer">
            <input
              type="radio"
              name={`filter-format-${strategyId}`}
              value="toml"
              checked={format === "toml"}
              onChange={() => setFormat("toml")}
              className="accent-gold"
            />
            <span className="font-mono">toml</span>
          </label>
          <label className="inline-flex items-center gap-1.5 cursor-pointer">
            <input
              type="radio"
              name={`filter-format-${strategyId}`}
              value="json"
              checked={format === "json"}
              onChange={() => setFormat("json")}
              className="accent-gold"
            />
            <span className="font-mono">json</span>
          </label>
        </div>

        <div className="flex flex-wrap items-center gap-3">
          <button
            type="button"
            onClick={() => saveMut.mutate()}
            disabled={!canSave}
            className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft disabled:opacity-40 disabled:hover:bg-gold transition-colors"
          >
            {saveMut.isPending ? "Saving…" : "Save filter"}
          </button>
          {hasFilter ? (
            <button
              type="button"
              onClick={() => clearMut.mutate()}
              disabled={busy}
              className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text-2 hover:text-text hover:border-text-3 disabled:opacity-40 transition-colors"
            >
              {clearMut.isPending ? "Clearing…" : "Clear filter"}
            </button>
          ) : null}
          {savedFlash ? (
            <span className="text-[12px] text-success">Saved.</span>
          ) : null}
        </div>

        {localError ? (
          <div className="text-[12px] text-danger font-mono whitespace-pre-wrap break-words">
            {localError}
          </div>
        ) : null}

        <div className="text-[11px] text-text-3">
          DSL reference: see <span className="font-mono">docs/operator/filters.md</span>
        </div>
      </div>
    </Card>
  );
}

function filterErrorMessage(err: unknown): string {
  if (err instanceof ApiError) {
    return err.field ? `${err.code} (${err.field}): ${err.message}` : `${err.code}: ${err.message}`;
  }
  if (err instanceof Error) return err.message;
  return String(err);
}
