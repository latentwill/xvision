import { Link } from "react-router-dom";

const SURFACES = [
  { slug: "run", label: "Run", note: "KlineCandlePane + RSI/MACD/ATR + Equity + Drawdown + Volume + MarkerDock" },
  { slug: "compare", label: "Compare", note: "UplotCompareOverlayPane + UplotDrawdownPane (worst-of)" },
  { slug: "scenario", label: "Scenario", note: "KlineCandlePane + Equity + Volume + MarkerDock" },
  { slug: "strategy", label: "Strategy", note: "KlineCandlePane + Live-vs-paper overlay + Drawdown" },
  { slug: "live", label: "Live", note: "KlineCandlePane + Equity + ConnectionStatus + CacheStatusBadge" },
  { slug: "wizard", label: "Wizard preview", note: "KlineCandlePane + Equity" },
];

export function ChartLabOverview() {
  return (
    <div className="grid gap-6 max-w-3xl">
      <section>
        <h2 className="text-[15px] font-medium text-text mb-2">Library split</h2>
        <p className="text-[13px] text-text-2">
          <strong>KlineCharts</strong> owns candles, candle-anchored overlays
          (SMA/EMA/Bollinger/Donchian) and candle-anchored markers
          (buy/sell/veto/hold). <strong>uPlot</strong> owns everything else —
          equity, drawdown, oscillators (RSI/MACD/ATR), compare overlays,
          histograms.
        </p>
      </section>

      <section>
        <h2 className="text-[15px] font-medium text-text mb-2">Surfaces</h2>
        <ul className="grid gap-2">
          {SURFACES.map((s) => (
            <li
              key={s.slug}
              className="border border-border rounded-card px-3 py-2 flex items-baseline gap-3"
            >
              <Link
                to={`/chart-lab/surfaces/${s.slug}`}
                className="text-[13px] font-medium text-text hover:text-gold"
              >
                {s.label}
              </Link>
              <span className="text-[12px] text-text-3">{s.note}</span>
            </li>
          ))}
        </ul>
      </section>

      <section>
        <h2 className="text-[15px] font-medium text-text mb-2">Spec</h2>
        <p className="text-[13px] text-text-2">
          <code className="text-[12px]">
            docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md
          </code>
          — M0 foundation (this PR) · M1 eval surfaces · M2 scenario+strategy ·
          M3 live+wizard · M4 v1 deletion.
        </p>
      </section>
    </div>
  );
}
