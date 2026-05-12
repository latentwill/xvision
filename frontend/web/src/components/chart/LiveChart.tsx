import { useState } from "react";

import { RunChart } from "./RunChart";
import { useRunStream, type LiveStatus } from "./use-run-stream";

type Props = {
  runId: string;
  themeMode?: "dark" | "light";
};

export function LiveChart({ runId, themeMode = "dark" }: Props) {
  const { data, status } = useRunStream(runId);
  const [follow, setFollow] = useState(true);

  return (
    <div>
      <div className="flex items-center justify-between text-[12px] mb-2">
        <span className="flex items-center gap-2">
          <StatusDot status={status} />
          <span className="text-text-3">{statusLabel(status)}</span>
        </span>
        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={follow}
            onChange={(e) => setFollow(e.target.checked)}
          />
          {follow ? "Following live" : "Frozen"}
          {!follow && (
            <button
              type="button"
              onClick={() => setFollow(true)}
              className="ml-2 underline"
            >
              Resume live
            </button>
          )}
        </label>
      </div>
      {data ? (
        <RunChart payload={data} themeMode={themeMode} />
      ) : (
        <div className="text-text-3 py-12 text-center">
          Waiting for first event…
        </div>
      )}
    </div>
  );
}

function StatusDot({ status }: { status: LiveStatus }) {
  const color =
    status === "streaming"
      ? "bg-green-500"
      : status === "reconnecting"
      ? "bg-amber-500"
      : status === "closed"
      ? "bg-red-500"
      : "bg-text-3";
  return <span className={`inline-block w-2 h-2 rounded-full ${color}`} />;
}

function statusLabel(s: LiveStatus): string {
  switch (s) {
    case "snapshot":
      return "loading snapshot…";
    case "streaming":
      return "live";
    case "reconnecting":
      return "reconnecting…";
    case "closed":
      return "closed";
  }
}
