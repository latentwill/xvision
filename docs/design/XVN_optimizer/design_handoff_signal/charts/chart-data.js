/* xvn — synthetic strategy + candle data for chart designs */

// xvn warm palette (must mirror chart-theme.css)
window.XVN_PALETTE = {
  gold:   '#00E676',
  amber:  '#38BDF8',
  bronze: '#5EEAD4',
  ember:  '#FB923C',
  copper: '#FF4D4D',
  plum:   '#C084FC',
  teal:   '#22D3EE',
  info:   '#5FA8FF',
  cream:  '#FFFFFF',
  mute:   '#5F6670',
  danger: '#FF4D4D',
};

// strategy rotation — gold leads, then warm hues, then cool counterpoint
// strategy rotation — gold leads, with deliberate hue+value spread for legibility
// gold → cream → orange → plum → teal → coral → bronze → benchmark gray
window.XVN_STRATEGIES = [
  { id:'fib',  name:'Fibonacci Golden Cross', short:'Fib · GC',    color:'#00E676', kind:'Trend',    return:+82.41, sharpe:1.92, mdd:-18.72, win:58.6, pf:1.81 },
  { id:'ema',  name:'EMA Pullback',           short:'EMA · 50/200',color:'#FBBF24', kind:'Trend',    return:+46.27, sharpe:1.41, mdd:-14.38, win:54.1, pf:1.46 },
  { id:'brk',  name:'Breakout Retest',        short:'BRK · 4h',    color:'#FB923C', kind:'Momentum', return:+28.14, sharpe:1.07, mdd:-12.93, win:51.2, pf:1.32 },
  { id:'msw',  name:'Momentum Swing',         short:'MSW · 1d',    color:'#A78BFA', kind:'Momentum', return:+12.68, sharpe:0.74, mdd:-15.91, win:47.8, pf:1.18 },
  { id:'mvr',  name:'Mean Reversion AI',      short:'MVR · 15m',   color:'#22D3EE', kind:'Reversion',return:+34.12, sharpe:1.18, mdd:-16.04, win:53.0, pf:1.39 },
  { id:'vsc',  name:'Volatility Scalper',     short:'VSC · 5m',    color:'#F472B6', kind:'Vol',      return:+21.85, sharpe:0.96, mdd:-11.42, win:50.7, pf:1.24 },
  { id:'lqh',  name:'Liquidation Hunter',     short:'LQH · 1h',    color:'#8B95A5', kind:'Vol',      return:+18.04, sharpe:0.81, mdd:-19.20, win:46.2, pf:1.16 },
  { id:'btc',  name:'BTC Buy & Hold',         short:'BTC · HOLD',  color:'#5F6670', kind:'Bench',    return:-3.21,  sharpe:0.22, mdd:-26.84, win:43.1, pf:0.89, dashed:true },
];

// ── deterministic PRNG so designs are repeatable ──
function mulberry32(seed){ return function(){ let t = seed += 0x6D2B79F5; t = Math.imul(t ^ t >>> 15, t | 1); t ^= t + Math.imul(t ^ t >>> 7, t | 61); return ((t ^ t >>> 14) >>> 0) / 4294967296; }; }

function makeEquity(strategy, points=260, base=0, vol=0.012, drift=0, seed=1){
  const rng = mulberry32(seed);
  const out = new Float64Array(points);
  let v = 0; // returns in %
  for (let i=0;i<points;i++){
    const shock = (rng()-0.5) * 2 * vol;
    v += drift + shock;
    // drag toward target return at end
    const target = strategy.return/100;
    v += (target * (i/points) - v) * 0.012;
    out[i] = v * 100;
  }
  // pin endpoints to nice values
  out[0] = 0;
  out[points-1] = strategy.return;
  return out;
}

function makeTime(points=260, startMs = Date.UTC(2024,0,2)){
  const day = 86400 * 1000;
  const out = new Float64Array(points);
  for (let i=0;i<points;i++) out[i] = (startMs + i * day) / 1000; // uPlot wants seconds
  return out;
}

window.makeEquitySeries = function(points=260){
  const time = makeTime(points);
  const series = {};
  for (const s of window.XVN_STRATEGIES){
    series[s.id] = makeEquity(s, points, 0, 0.010 + Math.random()*0.004, 0.0006, hash(s.id));
  }
  return { time, series };
};

function hash(str){ let h=2166136261; for (let i=0;i<str.length;i++){ h^=str.charCodeAt(i); h=Math.imul(h,16777619); } return h>>>0; }

window.makeDrawdownSeries = function(equity){
  const out = new Float64Array(equity.length);
  let peak = equity[0];
  for (let i=0;i<equity.length;i++){
    if (equity[i] > peak) peak = equity[i];
    out[i] = equity[i] - peak; // negative
  }
  return out;
};

// ── monthly returns matrix (rows = strategies, cols = months) ──
window.makeMonthlyMatrix = function(months=17){
  const rng = mulberry32(99);
  return window.XVN_STRATEGIES.slice(0,5).map(s => {
    const row = [];
    for (let i=0;i<months;i++){
      const base = s.return / 100 / 12;
      row.push(base + (rng()-0.5) * 0.10);
    }
    return { strategy: s, values: row };
  });
};

// ── candle data for AI annotation + liquidation charts ──
// KlineCharts v9 expects {timestamp:ms, open, high, low, close, volume}
window.makeCandles = function(count=160, startPrice=63500, seed=42){
  const rng = mulberry32(seed);
  const out = [];
  let price = startPrice;
  let t = Date.UTC(2025, 1, 1); // ms
  const dt = 3600 * 1000;       // 1h in ms
  for (let i=0;i<count;i++){
    const drift = (Math.sin(i/14) * 90) + (Math.cos(i/35) * 240);
    const open = price;
    const noise = (rng()-0.5) * 420;
    const close = open + noise + (drift - (i>0 ? (Math.sin((i-1)/14)*90 + Math.cos((i-1)/35)*240) : 0));
    const high = Math.max(open, close) + rng() * 280;
    const low  = Math.min(open, close) - rng() * 280;
    const vol  = 800 + rng() * 1800;
    out.push({ timestamp: t, time: t/1000, open, high, low, close, volume: vol });
    price = close;
    t += dt;
  }
  return out;
};

// ── liquidation heat (price levels with intensity) ──
window.makeLiquidationLevels = function(candles){
  const minP = Math.min(...candles.map(c=>c.low));
  const maxP = Math.max(...candles.map(c=>c.high));
  const range = maxP - minP;
  const rng = mulberry32(7);
  const levels = [];
  const buckets = 26;
  for (let i=0;i<buckets;i++){
    const p = minP + (i+0.5)/buckets * range;
    // hot near clusters; cooler at edges
    const center = 0.55 + Math.sin(i*0.6)*0.18;
    const dist = Math.abs((i/buckets) - center);
    const heat = Math.max(0, 1 - dist*1.6) * (0.4 + rng()*0.6);
    const notional = Math.round(heat * 280) / 1; // millions
    if (notional > 4) levels.push({ price: p, heat, notional, side: p > (minP + range*0.55) ? 'long' : 'short' });
  }
  return levels.sort((a,b)=>b.heat - a.heat);
};

// utility: fmt
window.fmtPct = (n, d=2) => (n>=0?'+':'') + n.toFixed(d) + '%';
window.fmtNum = (n, d=2) => n.toFixed(d);
window.fmt$ = (n) => '$' + n.toLocaleString(undefined,{ maximumFractionDigits: 0 });
