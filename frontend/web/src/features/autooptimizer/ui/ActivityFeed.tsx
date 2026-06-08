import { useEffect, useRef, useState } from "react";
import { formatEventLabel } from "../api";

interface FeedRow {
  id: number;
  time: string;
  label: string;
  hash?: string;
}

interface ActivityFeedProps {
  /** Filter events by this session id if provided (no filter = all sessions). */
  sessionId?: string;
  /** Maximum rows to keep in memory. Default 200. */
  maxRows?: number;
}

let nextId = 1;

function localSeqKey(sessionId: string) {
  return `aof:last_seq:${sessionId}`;
}

function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  } catch {
    return iso;
  }
}

/**
 * Live activity feed connected to /api/autooptimizer/events via EventSource.
 *
 * - Stores last seen seq in localStorage under `aof:last_seq:{sessionId}` so
 *   replays are not duplicated across page reloads.
 * - Auto-scrolls to bottom; shows "Jump to latest" when scrolled up.
 * - Layout rule: full-width, no right-side box (three-pane shell constraint).
 */
export function ActivityFeed({ sessionId, maxRows = 200 }: ActivityFeedProps) {
  const [rows, setRows] = useState<FeedRow[]>([]);
  const [atBottom, setAtBottom] = useState(true);
  const bottomRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Build SSE URL with since_seq resume if we have a stored seq
  const buildUrl = () => {
    const params = new URLSearchParams();
    if (sessionId) {
      const stored = localStorage.getItem(localSeqKey(sessionId));
      if (stored) params.set("since_seq", stored);
    }
    const qs = params.toString();
    return qs ? `/api/autooptimizer/events?${qs}` : "/api/autooptimizer/events";
  };

  useEffect(() => {
    const url = buildUrl();
    const source = new EventSource(url);

    const handleRaw = (ev: Event) => {
      const raw = (ev as MessageEvent).data;
      if (typeof raw !== "string" || !raw.trim()) return;

      let parsed: Record<string, unknown>;
      try {
        parsed = JSON.parse(raw) as Record<string, unknown>;
      } catch {
        return;
      }

      // Extract the inner data object if wrapped
      const data: Record<string, unknown> =
        typeof parsed.data === "object" && parsed.data !== null
          ? (parsed.data as Record<string, unknown>)
          : parsed;

      // Filter by sessionId when provided
      if (sessionId && data.session_id && data.session_id !== sessionId) return;

      const eventType =
        (typeof data.event_type === "string" ? data.event_type : null) ??
        (typeof data.type === "string" ? data.type : null) ??
        ev.type ??
        "";

      // Skip protocol noise
      if (eventType === "message" || eventType === "dropped") return;

      const label =
        typeof data.display_label === "string" && data.display_label
          ? data.display_label
          : formatEventLabel({ event_type: eventType, display_label: undefined });

      const ts =
        typeof data.ts === "string" ? data.ts : new Date().toISOString();

      const hash =
        typeof data.bundle_hash === "string" ? data.bundle_hash : undefined;

      // Persist last seq
      const seq = typeof data.seq === "number" ? data.seq : undefined;
      if (seq != null && sessionId) {
        localStorage.setItem(localSeqKey(sessionId), String(seq));
      }

      const row: FeedRow = { id: nextId++, time: ts, label, hash };
      setRows((prev) => {
        const next = prev.length >= maxRows ? prev.slice(1) : prev;
        return [...next, row];
      });
    };

    const EVENT_TYPES = [
      "cycle_started",
      "parent_selected",
      "mutation_proposed",
      "no_candidate",
      "mutation_gated",
      "mutation_accepted",
      "mutation_rejected",
      "honesty_check_run",
      "judge_finding",
      "cycle_finished",
      "diversity_scored",
      "job_started",
      "job_finished",
      "lagged",
    ];

    source.addEventListener("message", handleRaw);
    for (const name of EVENT_TYPES) source.addEventListener(name, handleRaw);

    return () => {
      source.removeEventListener("message", handleRaw);
      for (const name of EVENT_TYPES) source.removeEventListener(name, handleRaw);
      source.close();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId]);

  // Auto-scroll to bottom when new rows arrive (only when already at bottom)
  useEffect(() => {
    if (atBottom && typeof bottomRef.current?.scrollIntoView === "function") {
      bottomRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [rows.length, atBottom]);

  const handleScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    const threshold = 48;
    setAtBottom(el.scrollHeight - el.scrollTop - el.clientHeight < threshold);
  };

  const jumpToLatest = () => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
    setAtBottom(true);
  };

  return (
    <div data-testid="activity-feed" className="relative flex flex-col">
      {!atBottom && (
        <button
          type="button"
          onClick={jumpToLatest}
          className="absolute top-2 right-3 z-10 rounded border border-border bg-surface-card px-2.5 py-1 text-[12px] text-text-2 hover:text-text shadow-sm transition-colors"
        >
          Jump to latest ↓
        </button>
      )}
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="max-h-80 overflow-y-auto rounded-md border border-border bg-surface-card"
      >
        {rows.length === 0 ? (
          <p className="px-4 py-3 text-[13px] text-text-3">Waiting for events…</p>
        ) : (
          <table className="w-full text-[12px] border-collapse">
            <tbody>
              {rows.map((row) => (
                <tr
                  key={row.id}
                  className="border-b border-border/50 last:border-0 hover:bg-surface-elev/40"
                >
                  <td className="pl-3 pr-2 py-1.5 font-mono text-text-3 whitespace-nowrap w-24">
                    {formatTime(row.time)}
                  </td>
                  <td className="px-2 py-1.5 text-text-2">{row.label}</td>
                  {row.hash && (
                    <td className="px-2 py-1.5 font-mono text-text-3">
                      <a
                        href={`/optimizer/experiment/${row.hash}`}
                        className="hover:text-gold transition-colors"
                      >
                        {row.hash.slice(0, 8)}
                      </a>
                    </td>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        )}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
