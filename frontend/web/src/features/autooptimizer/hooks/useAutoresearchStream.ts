import { useCallback, useEffect, useRef, useState } from "react";

export type AutoresearchLogLine = {
  _row_id: number;
  text: string;
  ts: string;
};

let nextId = 1;

/** Subscribe to the SSE log stream for a running autoresearch run.
 *  Creates a new EventSource whenever `run_id` is non-null and the
 *  previous connection is absent. Cleans up on unmount or run_id change.
 *  Ring-buffered to 500 lines to avoid unbounded memory growth. */
export function useAutoresearchStream(run_id: string | null): {
  lines: AutoresearchLogLine[];
  connected: boolean;
} {
  const [lines, setLines] = useState<AutoresearchLogLine[]>([]);
  const [connected, setConnected] = useState(false);
  const sourceRef = useRef<EventSource | null>(null);

  const append = useCallback((text: string) => {
    setLines((prev) => {
      const row: AutoresearchLogLine = {
        _row_id: nextId++,
        text,
        ts: new Date().toISOString(),
      };
      const next = prev.length >= 500 ? prev.slice(1) : prev;
      return [...next, row];
    });
  }, []);

  useEffect(() => {
    if (!run_id) return;

    const src = new EventSource(
      `/api/autoresearch/runs/${encodeURIComponent(run_id)}/stream`,
    );
    sourceRef.current = src;

    src.addEventListener("open", () => setConnected(true));
    src.addEventListener("message", (ev: MessageEvent<string>) => {
      append(typeof ev.data === "string" ? ev.data : "");
    });
    src.addEventListener("error", () => setConnected(false));

    return () => {
      src.close();
      sourceRef.current = null;
      setConnected(false);
    };
  }, [run_id, append]);

  return { lines, connected };
}
