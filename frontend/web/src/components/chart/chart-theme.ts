// chart-theme.ts
export function chartTheme(mode: 'dark' | 'light') {
  return mode === 'dark' ? {
    background: '#0b0c0d', text: '#e6e6e6', grid: '#1a1d1f',
    series: { sma20: '#7dd3fc', sma50: '#fbbf24', sma200: '#f87171',
              ema20: '#a78bfa', ema50: '#fbbf24', ema200: '#f87171',
              bollUpper: '#34d399', bollMiddle: '#94a3b8', bollLower: '#34d399',
              donchianUpper: '#fb923c', donchianLower: '#fb923c',
              equity: '#22d3ee', drawdown: '#ef4444',
              candleUp: '#22c55e', candleDown: '#ef4444',
              positionLong: 'rgba(34,197,94,0.08)', positionShort: 'rgba(239,68,68,0.08)',
              markerBuy: '#22c55e', markerSell: '#ef4444', markerVeto: '#facc15', markerHold: '#94a3b8' },
  } : {
    background: '#fafafa', text: '#0b0c0d', grid: '#e5e7eb',
    series: { sma20: '#0284c7', sma50: '#a16207', sma200: '#b91c1c',
              ema20: '#7c3aed', ema50: '#a16207', ema200: '#b91c1c',
              bollUpper: '#15803d', bollMiddle: '#64748b', bollLower: '#15803d',
              donchianUpper: '#c2410c', donchianLower: '#c2410c',
              equity: '#0891b2', drawdown: '#dc2626',
              candleUp: '#16a34a', candleDown: '#dc2626',
              positionLong: 'rgba(34,197,94,0.1)', positionShort: 'rgba(239,68,68,0.1)',
              markerBuy: '#16a34a', markerSell: '#dc2626', markerVeto: '#ca8a04', markerHold: '#475569' },
  };
}
