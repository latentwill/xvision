// Strategies-folder API — wraps `/api/strategies-folder/*` on the
// dashboard. The folder is the per-user surface backing
// `$XVN_HOME/strategies/` (track 1: read; track 6 / this client: import).

import { apiFetch } from "./client";

export type FolderEntryKind =
  | "markdown"
  | "json"
  | "csv"
  | "pdf"
  | "text"
  | "other";

export type FolderEntry = {
  rel_path: string;
  kind: FolderEntryKind;
  size_bytes: number;
  modified_at: string;
};

export type ImportFinding = {
  code: string;
  detail: string;
};

export type ImportResponse = {
  entry: FolderEntry;
  summary?: FolderEntry | null;
  findings: ImportFinding[];
};

export async function listStrategiesFolder(
  subfolder?: string,
): Promise<FolderEntry[]> {
  const path = subfolder
    ? `/api/strategies-folder/list?subfolder=${encodeURIComponent(subfolder)}`
    : "/api/strategies-folder/list";
  const res = await apiFetch<{ items: FolderEntry[] }>(path);
  return res.items;
}

export type ImportFileOptions = {
  to?: string;
  noClobber?: boolean;
};

export async function importStrategiesFolderFile(
  file: File,
  options: ImportFileOptions = {},
): Promise<ImportResponse> {
  const form = new FormData();
  form.append("file", file, file.name);
  if (options.to) form.append("to", options.to);
  if (options.noClobber) form.append("no_clobber", "true");

  // Direct fetch — apiFetch sets a JSON content-type header by default,
  // which would break multipart boundary negotiation. Re-implement the
  // error-shape mapping inline to keep the surface consistent.
  const res = await fetch("/api/strategies-folder/import", {
    method: "POST",
    body: form,
  });
  if (!res.ok) {
    let message = `import failed (${res.status})`;
    try {
      const body = (await res.json()) as { message?: string; code?: string };
      if (body.message) message = body.message;
    } catch {
      // Ignore body-decode failures; fall back to the status-derived message.
    }
    throw new Error(message);
  }
  return (await res.json()) as ImportResponse;
}

export const strategiesFolderKeys = {
  all: ["strategies-folder"] as const,
  list: (subfolder?: string) =>
    [...strategiesFolderKeys.all, "list", subfolder ?? null] as const,
};
