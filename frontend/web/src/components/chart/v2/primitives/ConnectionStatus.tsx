type ConnectionState = "connected" | "reconnecting" | "offline";

type Props = {
  state: ConnectionState;
  lastTickMs?: number | null;
};

const STATE_CONFIG: Record<
  ConnectionState,
  { label: string; colorClass: string }
> = {
  connected: {
    label: "Live",
    colorClass: "text-green-500",
  },
  reconnecting: {
    label: "Reconnecting",
    colorClass: "text-amber-500",
  },
  offline: {
    label: "Offline",
    colorClass: "text-red-500",
  },
};

export function ConnectionStatus({ state, lastTickMs }: Props) {
  const config = STATE_CONFIG[state];
  const showTick = state === "connected" && lastTickMs != null;

  return (
    <span
      className={`inline-flex items-center gap-1.5 text-[11px] font-medium ${config.colorClass}`}
      aria-label={`Connection: ${config.label}`}
    >
      {/* Dot — pulsing when connected */}
      <span
        className={`inline-block w-1.5 h-1.5 rounded-full bg-current ${
          state === "connected" ? "animate-pulse" : ""
        }`}
        aria-hidden
      />
      <span>{config.label}</span>
      {showTick && (
        <span className="text-[10px] text-text-3 font-normal ml-0.5">
          {new Date(lastTickMs).toLocaleTimeString()}
        </span>
      )}
    </span>
  );
}
