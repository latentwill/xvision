import { useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { Topbar } from "@/components/shell/Topbar";
import {
  importStrategiesFolderFile,
  listStrategiesFolder,
  strategiesFolderKeys,
  type FolderEntry,
  type ImportFinding,
  type ImportResponse,
} from "@/api/strategies-folder";

const SUBFOLDER_LABELS: Record<string, string> = {
  notes: "Notes",
  docs: "Docs",
  "strategy-files": "Strategy files",
  evals: "Evals",
  library: "Library",
};

const SUBFOLDER_ORDER = [
  "notes",
  "docs",
  "strategy-files",
  "evals",
  "library",
];

type StatusEntry = {
  id: string;
  kind: "ok" | "error";
  filename: string;
  message: string;
  findings: ImportFinding[];
};

function topSubfolder(rel: string): string {
  const idx = rel.indexOf("/");
  return idx === -1 ? "" : rel.slice(0, idx);
}

function humanBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
}

/**
 * `/strategies-folder` — per-user import surface for
 * `$XVN_HOME/strategies/`. Renders the current contents grouped by
 * subfolder and a native file picker the operator uses to upload new
 * files. No popups: status feedback is inline.
 *
 * Spec: V2F wave-2 leaf `strategies-folder-import`. See the contract
 * at `team/contracts/strategies-folder-import.md`.
 */
export function StrategiesFolderRoute() {
  const qc = useQueryClient();
  const [statuses, setStatuses] = useState<StatusEntry[]>([]);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const entriesQuery = useQuery({
    queryKey: strategiesFolderKeys.list(),
    queryFn: () => listStrategiesFolder(),
  });

  const importer = useMutation({
    mutationFn: async (file: File): Promise<ImportResponse> =>
      importStrategiesFolderFile(file),
    onSuccess: (res, file) => {
      setStatuses((prev) => [
        {
          id: `${file.name}-${Date.now()}`,
          kind: "ok",
          filename: file.name,
          message: `Imported → ${res.entry.rel_path}${
            res.summary ? ` (sidecar: ${res.summary.rel_path})` : ""
          }`,
          findings: res.findings ?? [],
        },
        ...prev,
      ]);
      void qc.invalidateQueries({ queryKey: strategiesFolderKeys.all });
    },
    onError: (err: unknown, file) => {
      const message =
        err instanceof Error ? err.message : "Import failed";
      setStatuses((prev) => [
        {
          id: `${file.name}-${Date.now()}`,
          kind: "error",
          filename: file.name,
          message,
          findings: [],
        },
        ...prev,
      ]);
    },
  });

  const grouped = useMemo(() => {
    const entries = entriesQuery.data ?? [];
    const map = new Map<string, FolderEntry[]>();
    for (const e of entries) {
      const key = topSubfolder(e.rel_path) || "root";
      const bucket = map.get(key) ?? [];
      bucket.push(e);
      map.set(key, bucket);
    }
    return map;
  }, [entriesQuery.data]);

  const orderedKeys = useMemo(() => {
    const present = Array.from(grouped.keys());
    const known = SUBFOLDER_ORDER.filter((k) => present.includes(k));
    const extras = present
      .filter((k) => !SUBFOLDER_ORDER.includes(k))
      .sort();
    return [...known, ...extras];
  }, [grouped]);

  const onFilesPicked = (files: FileList | null) => {
    if (!files || files.length === 0) return;
    for (const file of Array.from(files)) {
      importer.mutate(file);
    }
    // Reset input so re-selecting the same file fires `change` again.
    if (fileInputRef.current) fileInputRef.current.value = "";
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <Topbar
        title="Strategies folder"
        sub="Notes, docs, and reference files the wizard can quote back to you."
      />

      <div className="flex flex-col gap-6 px-6 py-6">
        <section className="rounded border border-border-soft bg-surface-elev p-5">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="min-w-0">
              <h2 className="text-[15px] font-medium text-text">
                Import a file
              </h2>
              <p className="mt-1 text-[12.5px] text-text-3">
                Accepted: .md, .txt, .csv, .pdf, .json (max 25 MB each). The
                file lands in the right subfolder by extension; PDFs and
                CSVs also get a markdown summary sidecar.
              </p>
            </div>
            <label className="inline-flex cursor-pointer items-center gap-2 rounded border border-border bg-surface-panel px-3 py-1.5 text-[13px] text-text hover:border-gold/50 hover:text-gold">
              <input
                ref={fileInputRef}
                type="file"
                multiple
                accept=".md,.txt,.csv,.pdf,.json"
                onChange={(e) => onFilesPicked(e.target.files)}
                className="sr-only"
                data-testid="strategies-folder-file-input"
              />
              {importer.isPending ? "Uploading…" : "Choose files"}
            </label>
          </div>

          {statuses.length > 0 && (
            <ul
              className="mt-4 space-y-2"
              data-testid="strategies-folder-statuses"
            >
              {statuses.map((s) => (
                <li
                  key={s.id}
                  className={[
                    "rounded border px-3 py-2 text-[12.5px]",
                    s.kind === "ok"
                      ? "border-emerald-500/30 bg-emerald-500/5 text-text"
                      : "border-rose-500/30 bg-rose-500/5 text-text",
                  ].join(" ")}
                >
                  <div className="flex items-baseline justify-between gap-2">
                    <span className="font-medium">{s.filename}</span>
                    <span className="text-text-3">
                      {s.kind === "ok" ? "ok" : "error"}
                    </span>
                  </div>
                  <div className="mt-0.5 text-text-2">{s.message}</div>
                  {s.findings.length > 0 && (
                    <ul className="mt-1 list-disc pl-5 text-text-3">
                      {s.findings.map((f, i) => (
                        <li key={i}>
                          <code>{f.code}</code>: {f.detail}
                        </li>
                      ))}
                    </ul>
                  )}
                </li>
              ))}
            </ul>
          )}
        </section>

        <section className="rounded border border-border-soft bg-surface-elev p-5">
          <h2 className="text-[15px] font-medium text-text">Contents</h2>
          {entriesQuery.isLoading ? (
            <p className="mt-3 text-[12.5px] text-text-3">Loading…</p>
          ) : entriesQuery.isError ? (
            <p className="mt-3 text-[12.5px] text-rose-400">
              Failed to load folder contents.
            </p>
          ) : orderedKeys.length === 0 ? (
            <p className="mt-3 text-[12.5px] text-text-3">
              Nothing here yet. Upload a file or run{" "}
              <code>xvn strategies import &lt;path&gt;</code> from the CLI.
            </p>
          ) : (
            <div className="mt-3 space-y-5">
              {orderedKeys.map((key) => {
                const items = grouped.get(key) ?? [];
                return (
                  <div key={key}>
                    <h3 className="text-[12px] font-medium uppercase tracking-wider text-text-3">
                      {SUBFOLDER_LABELS[key] ?? key}
                    </h3>
                    <ul className="mt-2 divide-y divide-border-soft">
                      {items.map((e) => (
                        <li
                          key={e.rel_path}
                          className="flex items-baseline justify-between gap-4 py-1.5 text-[13px] text-text"
                        >
                          <span className="truncate font-mono text-[12.5px]">
                            {e.rel_path}
                          </span>
                          <span className="shrink-0 text-text-3">
                            {humanBytes(e.size_bytes)}
                          </span>
                        </li>
                      ))}
                    </ul>
                  </div>
                );
              })}
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
