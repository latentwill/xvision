/**
 * Pine Script Import Route — /strategies/import-pine
 *
 * Layout rules (from CLAUDE.md, mandatory):
 * - Single full-width column (space-y-5). NO right-side box / fourth column.
 * - NO popups / modals / sheets / overlays. Everything renders inline.
 * - Dark-mode-safe borders: border-border / border-border-soft only.
 *   Never border-white / border-gray-100 / border-gray-200.
 * - Colored status rows use low-opacity bg + dark: variants.
 */

import { useState, type FormEvent, type ChangeEvent } from "react";
import { Link } from "react-router-dom";
import { useMutation } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { ApiError } from "@/api/client";
import { importPineScript, type PineImportResult, type FidelityItem, type CostModel } from "@/api/pine-import";

// ─── helpers ────────────────────────────────────────────────────────────────

function errorDetail(err: unknown): string {
  if (err instanceof ApiError) return `${err.code}: ${err.message}`;
  if (err instanceof Error) return err.message;
  return String(err);
}

// ─── sub-components ─────────────────────────────────────────────────────────

function FidelitySection({
  title,
  items,
  tone,
}: {
  title: string;
  items: FidelityItem[];
  tone: "success" | "warn" | "danger";
}) {
  const toneClasses = {
    success: {
      header: "text-emerald-600 dark:text-emerald-400",
      row: "border-emerald-500/20 bg-emerald-500/[0.05] dark:bg-emerald-500/[0.07]",
      badge: "text-emerald-700 dark:text-emerald-300",
    },
    warn: {
      header: "text-amber-600 dark:text-amber-400",
      row: "border-amber-500/20 bg-amber-500/[0.05] dark:bg-amber-500/[0.07]",
      badge: "text-amber-700 dark:text-amber-300",
    },
    danger: {
      header: "text-rose-600 dark:text-rose-400",
      row: "border-rose-500/20 bg-rose-500/[0.05] dark:bg-rose-500/[0.07]",
      badge: "text-rose-700 dark:text-rose-300",
    },
  }[tone];

  return (
    <section aria-labelledby={`fidelity-${tone}-heading`}>
      <h3
        id={`fidelity-${tone}-heading`}
        className={`mb-2 text-[12px] font-semibold uppercase tracking-wide ${toneClasses.header}`}
      >
        {title} ({items.length})
      </h3>
      {items.length === 0 ? (
        <p className="text-[12px] text-text-3">None</p>
      ) : (
        <ul className="space-y-1.5">
          {items.map((entry) => (
            <li
              key={entry.item}
              className={`flex flex-col gap-0.5 rounded border px-3 py-2 ${toneClasses.row}`}
            >
              <span className={`font-mono text-[12px] font-semibold ${toneClasses.badge}`}>
                {entry.item}
              </span>
              <span className="text-[12px] text-text-2">{entry.reason}</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function CostModelStrip({ model }: { model: CostModel }) {
  return (
    <section
      aria-label="Cost model reference"
      className="rounded border border-border bg-surface-elev px-4 py-3 space-y-2"
    >
      <h3 className="text-[11px] font-semibold uppercase tracking-wide text-text-3">
        Cost model reference
      </h3>
      <div className="flex flex-wrap gap-x-6 gap-y-1.5">
        <span className="text-[12px] text-text-2">
          <span className="font-medium text-text">Commission:</span>{" "}
          {model.commission_type} · {model.commission_value_bps} bps
        </span>
        <span className="text-[12px] text-text-2">
          <span className="font-medium text-text">Slippage:</span>{" "}
          {model.slippage_model} · {model.slippage_value_bps} bps
        </span>
        <span className="text-[12px] text-text-2">
          <span className="font-medium text-text">Fill timing:</span>{" "}
          {model.fill_timing}
        </span>
      </div>
      {model.note ? (
        <p className="text-[11.5px] text-text-3 leading-relaxed">{model.note}</p>
      ) : null}
    </section>
  );
}

function FidelityDiff({ result }: { result: PineImportResult }) {
  const { fidelity_report, strategy } = result;
  const strategyId = strategy.manifest.id;

  return (
    <div className="space-y-5">
      {/* strategy link + optimize CTA — full-width inline row */}
      <div className="flex flex-wrap items-center gap-3 rounded border border-border bg-surface-card px-4 py-3">
        <span className="text-[13px] font-medium text-text">
          {strategy.manifest.display_name || "Imported strategy"}
        </span>
        <Link
          to={`/strategies/${encodeURIComponent(strategyId)}`}
          className="text-[12.5px] font-medium text-text-2 underline underline-offset-2 hover:text-text"
          aria-label="View strategy"
        >
          View strategy
        </Link>
        <Link
          to={`/optimizer?strategy=${encodeURIComponent(strategyId)}`}
          className="inline-flex items-center gap-1.5 rounded bg-gold px-3 py-1.5 text-[12.5px] font-medium text-bg transition-colors hover:bg-gold-soft"
          aria-label="Optimize this"
        >
          Optimize this
        </Link>
      </div>

      {/* fidelity diff — three sections inline */}
      <Card className="p-4 space-y-5">
        <h2 className="m-0 text-[15px] font-semibold tracking-tight text-text">
          Fidelity report
        </h2>
        <FidelitySection
          title="Captured"
          items={fidelity_report.captured}
          tone="success"
        />
        <FidelitySection
          title="Approximated"
          items={fidelity_report.approximated}
          tone="warn"
        />
        <FidelitySection
          title="Dropped"
          items={fidelity_report.dropped}
          tone="danger"
        />
      </Card>

      {/* cost model reference strip */}
      <CostModelStrip model={fidelity_report.cost_model} />
    </div>
  );
}

// ─── main route ─────────────────────────────────────────────────────────────

export function StrategiesImportPineRoute() {
  const [source, setSource] = useState("");
  const [name, setName] = useState("");

  const importMutation = useMutation<PineImportResult, unknown, { source: string; name?: string }>({
    mutationFn: ({ source, name }) =>
      importPineScript({ source, name: name || undefined }),
  });

  function onSubmit(e: FormEvent) {
    e.preventDefault();
    if (importMutation.isPending || source.trim().length === 0) return;
    importMutation.mutate({ source: source.trim(), name: name || undefined });
  }

  return (
    <>
      <Topbar title="Import Pine Script" />

      {/* single-column layout, no right-side box */}
      <div className="space-y-5">
        {/* import form */}
        <Card className="p-5 max-w-2xl">
          <form onSubmit={onSubmit} className="space-y-4">
            <div>
              <h2 className="m-0 font-sans font-medium text-[20px] tracking-tight text-text">
                Import from Pine Script
              </h2>
              <p className="m-0 mt-1 text-[12px] text-text-3">
                Paste a TradingView Pine Script v5 strategy. xvision will parse
                the supported subset and create a strategy you can immediately
                evolve with the optimizer.
              </p>
            </div>

            <div>
              <label
                htmlFor="pine-name"
                className="block text-[12px] text-text-2 mb-1"
              >
                Strategy name{" "}
                <span className="text-text-3">(optional)</span>
              </label>
              <input
                id="pine-name"
                type="text"
                value={name}
                onChange={(e: ChangeEvent<HTMLInputElement>) => setName(e.target.value)}
                placeholder="My imported strategy"
                disabled={importMutation.isPending}
                aria-label="Strategy name"
                className="w-full px-3 py-2 bg-surface-elev border border-border rounded text-text text-[13px] focus:outline-none focus:border-text-3 disabled:opacity-50"
              />
            </div>

            <div>
              <label
                htmlFor="pine-source"
                className="block text-[12px] text-text-2 mb-1"
              >
                Pine Script source
              </label>
              <textarea
                id="pine-source"
                value={source}
                onChange={(e: ChangeEvent<HTMLTextAreaElement>) => setSource(e.target.value)}
                placeholder={`//@version=5\nstrategy("My Strategy", overlay=true)\n// paste your Pine Script here…`}
                rows={12}
                disabled={importMutation.isPending}
                aria-label="Pine Script source"
                className="w-full px-3 py-2 bg-surface-elev border border-border rounded text-text font-mono text-[12.5px] focus:outline-none focus:border-text-3 disabled:opacity-50 resize-y"
              />
            </div>

            {/* inline error — no overlay */}
            {importMutation.isError ? (
              <div
                role="alert"
                className="rounded border border-rose-500/30 bg-rose-500/[0.06] px-3 py-2 space-y-1"
              >
                <div className="text-[13px] font-semibold text-rose-600 dark:text-rose-400">
                  Import failed
                </div>
                <code className="text-[12px] font-mono text-rose-600/80 dark:text-rose-400/80 break-all">
                  {errorDetail(importMutation.error)}
                </code>
              </div>
            ) : null}

            <div className="flex items-center justify-between gap-2 pt-1">
              <Link
                to="/strategies"
                className="text-[13px] text-text-3 hover:text-text"
              >
                Cancel
              </Link>
              <button
                type="submit"
                disabled={importMutation.isPending || source.trim().length === 0}
                className="px-3.5 py-1.5 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
              >
                {importMutation.isPending ? "Importing…" : "Import"}
              </button>
            </div>
          </form>
        </Card>

        {/* fidelity diff — rendered inline below the form on success */}
        {importMutation.isSuccess && importMutation.data ? (
          <FidelityDiff result={importMutation.data} />
        ) : null}
      </div>
    </>
  );
}
