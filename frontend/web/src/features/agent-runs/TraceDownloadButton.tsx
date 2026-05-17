// frontend/web/src/features/agent-runs/TraceDownloadButton.tsx
//
// Toolbar control for the trace dock: fetches the per-run JSON export
// endpoint (added in #226 — `GET /api/agent-runs/:id/export.json`) and
// triggers a browser download via the standard Blob + URL.createObjectURL
// + hidden `<a download>` click pattern.
//
// Filename defaults to `xvn_run_<runId>.json` to match the backend's
// `Content-Disposition: attachment; filename=...` default, but if the
// server provides one we use that verbatim.

import { useState } from "react";
import { agentRunExportUrl } from "@/api/agent-runs";

/**
 * Parse a Content-Disposition header for the `filename` value.
 * Handles both `filename="quoted"` and bare `filename=token` forms,
 * and the RFC 5987 `filename*=UTF-8''encoded` form (lowest priority — we
 * fall back to the unencoded value if both are present, since the encoded
 * form is rare for ASCII run IDs).
 */
export function filenameFromContentDisposition(header: string | null): string | null {
  if (!header) return null;
  // Prefer the simple `filename=` value; only fall back to `filename*=` when absent.
  const simple = /filename\s*=\s*("([^"]+)"|([^;]+))/i.exec(header);
  if (simple) {
    const value = (simple[2] ?? simple[3] ?? "").trim();
    if (value) return value;
  }
  const star = /filename\*\s*=\s*[^']*'[^']*'([^;]+)/i.exec(header);
  if (star) {
    try {
      return decodeURIComponent(star[1].trim());
    } catch {
      return star[1].trim();
    }
  }
  return null;
}

export function TraceDownloadButton({
  runId,
  className,
}: {
  runId: string;
  className?: string;
}) {
  const [busy, setBusy] = useState(false);

  async function onClick() {
    if (busy) return;
    setBusy(true);
    const url = agentRunExportUrl(runId);
    try {
      const res = await fetch(url, {
        credentials: "include",
        headers: { accept: "application/json" },
      });
      if (!res.ok) {
        // No-popups rule: surface failure via console.warn. A future
        // qa-trace-error-surfacing pass can lift this to a toast.
        console.warn("[agent-runs] trace-download failed", {
          runId,
          status: res.status,
          statusText: res.statusText,
        });
        return;
      }
      const filename =
        filenameFromContentDisposition(res.headers.get("content-disposition")) ??
        `xvn_run_${runId}.json`;
      const blob = await res.blob();
      const objectUrl = URL.createObjectURL(blob);
      try {
        const a = document.createElement("a");
        a.href = objectUrl;
        a.download = filename;
        a.rel = "noopener";
        // Append + click is the most reliable cross-browser pattern.
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
      } finally {
        URL.revokeObjectURL(objectUrl);
      }
    } catch (err) {
      console.warn("[agent-runs] trace-download error", {
        runId,
        error: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setBusy(false);
    }
  }

  return (
    <button
      type="button"
      onClick={onClick}
      disabled={busy}
      aria-label="download trace JSON"
      title="Download full trace as JSON"
      data-testid="trace-download-button"
      className={
        className ??
        "px-1.5 py-0.5 border border-border rounded-sm hover:opacity-80 disabled:opacity-40"
      }
    >
      {busy ? "…" : "⬇ json"}
    </button>
  );
}
