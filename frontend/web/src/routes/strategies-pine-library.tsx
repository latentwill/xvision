/**
 * Pine Script Library Route — /strategies/pine-library
 *
 * Browsable seed library: shows name/description for each curated entry
 * with a one-click "Import → Optimize" action.
 *
 * Layout rules (from CLAUDE.md, mandatory):
 * - Single full-width column (space-y-5). NO right-side box / fourth column.
 * - NO popups / modals / sheets / overlays. Everything renders inline.
 * - Dark-mode-safe borders: border-border / border-border-soft only.
 *   Never border-white / border-gray-100 / border-gray-200.
 * - Colored status rows use low-opacity bg + dark: variants.
 */

import { Link, useNavigate } from "react-router-dom";
import { useQuery, useMutation } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { ApiError } from "@/api/client";
import {
  getPineLibrary,
  importLibraryEntry,
  type LibraryEntrySummary,
} from "@/api/pine-library";
import type { PineImportResult } from "@/api/pine-import";

// ─── helpers ────────────────────────────────────────────────────────────────

function errorDetail(err: unknown): string {
  if (err instanceof ApiError) return `${err.code}: ${err.message}`;
  if (err instanceof Error) return err.message;
  return String(err);
}

// ─── sub-components ─────────────────────────────────────────────────────────

/** Single library entry card with inline import action and result. */
function LibraryCard({
  entry,
  onImportSuccess,
}: {
  entry: LibraryEntrySummary;
  onImportSuccess?: (id: string, result: PineImportResult) => void;
}) {
  const navigate = useNavigate();

  const importMutation = useMutation<PineImportResult, unknown, string>({
    mutationFn: (id) => importLibraryEntry(id),
    onSuccess: (result, id) => {
      // Route to the created strategy with an Optimize CTA.
      const strategyId = result.strategy.manifest.id;
      if (onImportSuccess) onImportSuccess(id, result);
      // Navigate to the strategy detail page.
      navigate(`/strategies/${encodeURIComponent(strategyId)}`);
    },
  });

  return (
    <Card
      className="p-4 space-y-3"
      aria-label={`Library entry: ${entry.name}`}
    >
      {/* Entry header */}
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 flex-1 space-y-0.5">
          <h3 className="m-0 text-[14px] font-semibold tracking-tight text-text">
            {entry.name}
          </h3>
          <p className="m-0 text-[12px] text-text-2 leading-relaxed">
            {entry.description}
          </p>
        </div>

        {/* Import action */}
        <button
          type="button"
          disabled={importMutation.isPending}
          onClick={() => importMutation.mutate(entry.id)}
          aria-label={`Import ${entry.name}`}
          className="shrink-0 inline-flex items-center gap-1.5 rounded bg-gold px-3 py-1.5 text-[12.5px] font-medium text-bg transition-colors hover:bg-gold-soft disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {importMutation.isPending ? "Importing…" : "Import → Optimize"}
        </button>
      </div>

      {/* Inline import error (no overlay) */}
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
    </Card>
  );
}

// ─── main route ─────────────────────────────────────────────────────────────

export function StrategiesPineLibraryRoute() {
  const {
    data,
    isLoading,
    isError,
    error,
  } = useQuery({
    queryKey: ["pine-library"],
    queryFn: getPineLibrary,
  });

  return (
    <>
      <Topbar title="Pine Script Library" />

      {/* single-column layout, no right-side box */}
      <div className="space-y-5">
        {/* page header */}
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="m-0 font-sans font-medium text-[20px] tracking-tight text-text">
              Starter strategy library
            </h2>
            <p className="m-0 mt-1 text-[12px] text-text-2">
              Click{" "}
              <span className="font-medium text-text">Import → Optimize</span>{" "}
              to create a strategy and immediately evolve it with the optimizer.
            </p>
          </div>
          <Link
            to="/strategies/import-pine"
            className="text-[13px] text-text-3 hover:text-text underline underline-offset-2"
          >
            Import custom Pine script
          </Link>
        </div>

        {/* loading state */}
        {isLoading ? (
          <p className="text-[13px] text-text-3">Loading library…</p>
        ) : null}

        {/* error state */}
        {isError ? (
          <div
            role="alert"
            className="rounded border border-rose-500/30 bg-rose-500/[0.06] px-4 py-3"
          >
            <div className="text-[13px] font-semibold text-rose-600 dark:text-rose-400">
              Failed to load library
            </div>
            <code className="mt-1 text-[12px] font-mono text-rose-600/80 dark:text-rose-400/80 break-all">
              {errorDetail(error)}
            </code>
          </div>
        ) : null}

        {/* empty library state */}
        {data && data.items.length === 0 ? (
          <div className="rounded border border-border bg-surface-elev px-4 py-6 text-center">
            <p className="m-0 text-[13px] text-text-3">
              No library entries available.
            </p>
          </div>
        ) : null}

        {/* library grid */}
        {data && data.items.length > 0 ? (
          <div className="space-y-3">
            {data.items.map((entry) => (
              <LibraryCard key={entry.id} entry={entry} />
            ))}
          </div>
        ) : null}
      </div>
    </>
  );
}
