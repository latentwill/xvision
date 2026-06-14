import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useRiver } from "../api";
import { useCycleEventStream } from "../hooks/useCycleEventStream";
import { buildBoardState } from "../selectors/buildBoardState";
import {
  buildRiverLayout,
  type RiverStub,
  type RiverPoint,
} from "../selectors/buildRiverLayout";
import { ExpandableArtifact } from "./ExpandableArtifact";

const W = 640;
const H = 220;
const PAD = 24;

type Hover =
  | { kind: "stub"; stub: RiverStub }
  | { kind: "point"; point: RiverPoint; champion: boolean }
  | null;

export function LineageRiver({ hasHistory = false }: { hasHistory?: boolean }) {
  const stream = useCycleEventStream();
  const river = useRiver({ refetchIntervalWhileRunning: stream.isRunning });
  const layout = useMemo(() => buildRiverLayout(river.data ?? []), [river.data]);
  const board = useMemo(
    () => buildBoardState(stream.isRunning ? stream.events : []),
    [stream.events, stream.isRunning],
  );
  const inflight = board.cards.filter(
    (c) => c.state === "evaluating" || c.state === "queued",
  );
  const [hover, setHover] = useState<Hover>(null);
  // lastHovered is never cleared — the readout stays populated after the mouse
  // leaves the SVG so the user can move to the "Open cycle →" button.
  const [lastHovered, setLastHovered] = useState<Hover>(null);
  const [pinned, setPinned] = useState<Hover>(null);
  const navigate = useNavigate();

  if (!river.data || river.data.length === 0) {
    if (!hasHistory) return null;
    return (
      <section className="rounded-md border border-border bg-surface-card p-5">
        <div className="text-[11px] uppercase tracking-widest text-text-4">
          Lineage · Sharpe over generations
        </div>
        <p className="mt-2 text-[12px] text-text-3">No lineage recorded yet.</p>
      </section>
    );
  }

  const sx = (x: number) =>
    PAD + (layout.xMax === 0 ? 0 : (x / layout.xMax) * (W - 2 * PAD));
  const [y0, y1] = layout.yDomain;
  const sy = (y: number) =>
    H - PAD - (y1 === y0 ? 0.5 : (y - y0) / (y1 - y0)) * (H - 2 * PAD);
  const keptCount = layout.lines.reduce((n, l) => n + l.points.length, 0);
  const active = pinned ?? lastHovered;

  return (
    <section className="rounded-md border border-border bg-surface-card p-5 space-y-3">
      <div className="text-[11px] uppercase tracking-widest text-text-4">
        Lineage · Sharpe over generations
        {keptCount <= 0 && (
          <span className="ml-2 normal-case tracking-normal text-text-3">
            {layout.lines.length} line{layout.lines.length === 1 ? "" : "s"} · nothing
            kept yet
          </span>
        )}
      </div>
      <svg
        role="img"
        aria-label="Lineage river"
        viewBox={`0 0 ${W} ${H}`}
        className="w-full"
        onMouseLeave={() => setHover(null) /* only clears in-chart highlight; readout stays */}
      >
        {layout.stubs.map((s) => (
          <line
            key={s.hash}
            data-testid="river-stub"
            data-hash={s.hash}
            x1={sx(s.fromX)}
            y1={sy(s.fromY)}
            x2={sx(s.fromX) + 26}
            y2={sy(s.y)}
            className={s.kind === "suspect" ? "stroke-warn/70" : "stroke-border-strong"}
            opacity={0.35 + 0.65 * s.ageRank} // fade with age: oldest stubs are faintest
            strokeDasharray={s.kind === "suspect" ? "3 2" : undefined}
            strokeWidth={hover?.kind === "stub" && hover.stub.hash === s.hash ? 2.5 : 1.2}
            onMouseOver={() => { const h = { kind: "stub" as const, stub: s }; setHover(h); setLastHovered(h); }}
            onClick={() => setPinned({ kind: "stub", stub: s })}
            style={{ cursor: "pointer" }}
          />
        ))}
        {layout.lines.map((l, i) => (
          <g key={i}>
            <path
              data-testid="river-line"
              d={l.points
                .map((p, j) => `${j === 0 ? "M" : "L"}${sx(p.x)},${sy(p.y)}`)
                .join(" ")}
              fill="none"
              className={
                !l.alive
                  ? "stroke-text-4/40"
                  : l.champion
                    ? "stroke-gold"
                    : "stroke-gold-soft/60"
              }
              strokeWidth={l.champion ? 2.4 : 1.5}
            />
            {l.points.map((p) => (
              <circle
                key={p.hash}
                data-hash={p.hash}
                cx={sx(p.x)}
                cy={sy(p.y)}
                r={l.champion ? 3.5 : 2.5}
                className={l.champion ? "fill-gold" : "fill-gold-soft"}
                onMouseOver={() => { const h = { kind: "point" as const, point: p, champion: l.champion }; setHover(h); setLastHovered(h); }}
                onClick={() => setPinned({ kind: "point", point: p, champion: l.champion })}
                style={{ cursor: "pointer" }}
              />
            ))}
            {l.alive && l.points.length > 0 && (
              <circle
                data-testid="river-live-end"
                role="link"
                aria-label={`Open strategy ${l.points.at(-1)!.hash}`}
                cx={sx(l.points.at(-1)!.x)}
                cy={sy(l.points.at(-1)!.y)}
                r={6}
                className="fill-transparent"
                tabIndex={0}
                onClick={() => navigate(`/optimizer/strategy/${l.points.at(-1)!.hash}`)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") navigate(`/optimizer/strategy/${l.points.at(-1)!.hash}`);
                }}
                style={{ cursor: "pointer" }}
              />
            )}
          </g>
        ))}
        {stream.isRunning &&
          layout.lines.some((l) => l.champion) &&
          (() => {
            const tip = layout.lines.find((l) => l.champion)!.points.at(-1)!;
            return (
              <g data-testid="river-frontier">
                <circle cx={sx(tip.x)} cy={sy(tip.y)} r={5} className="fill-gold" opacity={0.3}>
                  <animate attributeName="r" values="5;9;5" dur="2s" repeatCount="indefinite" />
                </circle>
                {inflight.map((c, k) => (
                  <line
                    key={c.hash}
                    data-testid="river-ghost"
                    x1={sx(tip.x)}
                    y1={sy(tip.y)}
                    x2={sx(tip.x) + 30}
                    y2={sy(tip.y) + (k - inflight.length / 2) * 12}
                    className="stroke-gold/40"
                    strokeDasharray="2 3"
                  />
                ))}
              </g>
            );
          })()}
      </svg>
      <RiverReadout
        active={active}
        expanded={pinned != null}
        onOpenCycle={(cycleId, hash) => navigate(`/optimizer/cycle/${cycleId}?exp=${hash}`)}
      />
    </section>
  );
}

function RiverReadout({
  active,
  expanded,
  onOpenCycle,
}: {
  active: Hover;
  expanded: boolean;
  onOpenCycle: (cycleId: string, hash: string) => void;
}) {
  if (!active)
    return (
      <div className="rounded-sm border border-border-soft px-3 py-2 font-mono text-[11px] text-text-4">
        hover a branch…
      </div>
    );
  const hash = active.kind === "stub" ? active.stub.hash : active.point.hash;
  const cycleId = active.kind === "stub" ? active.stub.cycleId : active.point.cycleId;
  const summary =
    active.kind === "stub" ? (
      <span className="font-mono text-[11px]">
        {hash.slice(0, 8)} ·{" "}
        <span className={active.stub.kind === "suspect" ? "text-warn" : "text-danger"}>
          {active.stub.kind === "suspect" ? "Suspect" : "Rejected"}
        </span>
        {active.stub.delta != null &&
          ` · ΔSharpe ${active.stub.delta >= 0 ? "+" : "−"}${Math.abs(active.stub.delta).toFixed(2)}`}
      </span>
    ) : (
      <span className="font-mono text-[11px]">
        {hash.slice(0, 8)} ·{" "}
        <span className="text-gold">{active.champion ? "Champion" : "Kept"}</span> ·
        Sharpe {active.point.y.toFixed(2)}
      </span>
    );
  return (
    <div className="space-y-1">
      <ExpandableArtifact
        key={`${hash}-${expanded ? "pinned" : "hover"}`}
        hash={hash}
        summary={summary}
        defaultOpen={expanded}
      />
      {cycleId && (
        <button
          type="button"
          onClick={() => onOpenCycle(cycleId, hash)}
          className="text-[11px] text-gold hover:underline"
        >
          Open cycle →
        </button>
      )}
    </div>
  );
}
