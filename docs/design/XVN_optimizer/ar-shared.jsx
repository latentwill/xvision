// Shared data + components for the Autoresearch surfaces.
// All four frames (home / cycle / variant / settings) read from the same
// fixture so the story stays consistent across the canvas.

// =============================================================================
//  FIXTURE
// =============================================================================

const AR_REGIMES = [
  { id:"bull-q1-25",       label:"bull-q1-25",       kind:"bull",  window:"jan–mar 2025", bars:5760 },
  { id:"chop-q2-25",       label:"chop-q2-25",       kind:"chop",  window:"apr–jun 2025", bars:5832 },
  { id:"bear-q3-24",       label:"bear-q3-24",       kind:"bear",  window:"jul–sep 2024", bars:5808 },
  { id:"flash-crash-24-08",label:"flash-crash-24-08",kind:"shock", window:"08 aug 2024",  bars:288  },
  { id:"chop-q4-23",       label:"chop-q4-23",       kind:"chop",  window:"oct–dec 2023", bars:5810 },
];

// Population — strategy lineages the autoresearcher is currently running experiments.
const AR_POPULATION = [
  { lineage:"btc-momentum",   parent:"btc-momentum-v3",       parentSharpe:1.31, variants:14, kept:2, status:"breeding",
    seed:"btc-momentum-7a91-v3",   model:"Claude · Haiku 4.5", assets:["BTC"] },
  { lineage:"eth-mean-rev",   parent:"eth-mr-v3",             parentSharpe:1.62, variants:12, kept:1, status:"breeding",
    seed:"eth-mr-3b22-v3",         model:"Claude · Haiku 4.5", assets:["ETH"] },
  { lineage:"sol-strategist", parent:"sol-strategist-v4.1",   parentSharpe:1.78, variants:10, kept:3, status:"breeding",
    seed:"sol-strategist-12fa-v4", model:"Claude · Sonnet 4.5",assets:["SOL"] },
  { lineage:"multi-asset",    parent:"multi-asset-v2",        parentSharpe:1.41, variants:8,  kept:0, status:"breeding",
    seed:"multi-asset-9c10-v2",    model:"GPT-5",              assets:["BTC","ETH","SOL"] },
  { lineage:"btc-grid",       parent:"btc-grid-v2",           parentSharpe:1.08, variants:6,  kept:1, status:"cooled",
    seed:"btc-grid-6f5b-v2",       model:"Claude · Haiku 4.5", assets:["BTC"] },
  { lineage:"meme-radar",     parent:"meme-radar-v1",         parentSharpe:0.62, variants:4,  kept:0, status:"paused",
    seed:"meme-radar-de44-v1",     model:"GPT-5",              assets:["SOL"] },
];

// Experiment kinds — what the autoresearcher actually changes between parent → variant.
// (operator-facing: "experiment kinds" — internal var keeps `AR_EXPERIMENTS` per terminology lock §1)
const AR_EXPERIMENTS = {
  "prompt-tweak":      { label:"Prompt tweak",      desc:"rewrites a section of system prompt",     tone:"info"  },
  "threshold-tune":    { label:"Threshold tune",    desc:"adjusts numeric gates (entry, stop, tp)", tone:"violet"},
  "agent-add":         { label:"Agent +",           desc:"adds a sub-agent or skill",               tone:"gold"  },
  "agent-remove":      { label:"Agent −",           desc:"removes a sub-agent or skill",            tone:"warn"  },
  "regime-detect-swap":{ label:"Regime detect swap",desc:"swaps regime classifier",                 tone:"info"  },
  "model-swap":        { label:"Model swap",        desc:"swaps Stage-2 model",                     tone:"violet"},
};

// Tonight's cycle — 14 variants of btc-momentum-v3.
const AR_CURRENT_CYCLE = {
  id: "cyc-01N8R2K9",
  lineage: "btc-momentum",
  parent: "btc-momentum-v3",
  parentSeed: "btc-momentum-7a91-v3",
  parentSharpe: 1.31,
  startedAt: "23:00 · 2026-05-26",
  expectedEndAt: "06:14 · 2026-05-27",
  elapsed: "4h 12m",
  remaining: "3h 02m",
  progress: 0.58,
  evalsTotal: 70,      // 14 variants × 5 regimes
  evalsDone:  41,
  evalsFailed: 1,
  llmCalls: "14,820",
  tokensSpent: "8.42M",
  costUSD: "$4.20",     // running LLM cost this cycle (no gas — all local backtest)
  // 14 variants — mix of statuses, experiment kinds, gate outcomes
  variants: [
    { id:"v3.1.a", experiment:"prompt-tweak",       gate:"PASS", deltaSharpe:+0.18, sharpe:1.49, status:"done",
      attestations:{endorse:2, question:0, reject:0}, kept:true,  seed:"btc-momentum-7a91-v3-1a",
      summary:"Tightens regime-detect prompt; reduces ambiguous-bar count by 18%." },
    { id:"v3.1.b", experiment:"threshold-tune",     gate:"PASS", deltaSharpe:+0.11, sharpe:1.42, status:"done",
      attestations:{endorse:2, question:0, reject:0}, kept:true,  seed:"btc-momentum-7a91-v3-1b",
      summary:"Lifts stop-loss to 1.4%; cuts whipsaw exits in chop." },
    { id:"v3.1.c", experiment:"prompt-tweak",       gate:"FAIL", deltaSharpe:-0.34, sharpe:0.97, status:"done",
      attestations:{endorse:0, question:1, reject:1}, kept:false, seed:"btc-momentum-7a91-v3-1c",
      summary:"Adds 'be more aggressive in trends' clause; overfits bull, dies in chop." },
    { id:"v3.1.d", experiment:"agent-add",          gate:"PASS", deltaSharpe:+0.07, sharpe:1.38, status:"done",
      attestations:{endorse:2, question:0, reject:0}, kept:false, seed:"btc-momentum-7a91-v3-1d",
      summary:"Adds funding-rate sub-agent; marginal lift, kept for breeding." },
    { id:"v3.1.e", experiment:"threshold-tune",     gate:"WARN", deltaSharpe:+0.04, sharpe:1.35, status:"done",
      attestations:{endorse:1, question:1, reject:0}, kept:false, seed:"btc-momentum-7a91-v3-1e",
      summary:"Δ-Sharpe positive but diversity-check fails embedding distance." },
    { id:"v3.1.f", experiment:"agent-remove",       gate:"FAIL", deltaSharpe:-0.61, sharpe:0.70, status:"done",
      attestations:{endorse:0, question:0, reject:2}, kept:false, seed:"btc-momentum-7a91-v3-1f",
      summary:"Drops regime classifier — collapses on flash-crash regime." },
    { id:"v3.1.g", experiment:"prompt-tweak",       gate:"PASS", deltaSharpe:+0.22, sharpe:1.53, status:"done",
      attestations:{endorse:2, question:0, reject:0}, kept:true,  seed:"btc-momentum-7a91-v3-1g",
      summary:"Reframes Stage-2 risk-cap reasoning; biggest Δ in cycle so far." },
    { id:"v3.1.h", experiment:"model-swap",         gate:"WARN", deltaSharpe:+0.01, sharpe:1.32, status:"done",
      attestations:{endorse:1, question:1, reject:0}, kept:false, seed:"btc-momentum-7a91-v3-1h",
      summary:"Sonnet 4.5 vs Haiku 4.5 — no significant improvement at 3× cost." },
    { id:"v3.1.i", experiment:"regime-detect-swap", gate:"…",    deltaSharpe:null,  sharpe:null, status:"running",
      attestations:{endorse:0, question:0, reject:0}, kept:false, seed:"btc-momentum-7a91-v3-1i",
      summary:"Running · 3 of 5 regimes complete · current run flash-crash-24-08." },
    { id:"v3.1.j", experiment:"threshold-tune",     gate:"…",    deltaSharpe:null,  sharpe:null, status:"running",
      attestations:{endorse:0, question:0, reject:0}, kept:false, seed:"btc-momentum-7a91-v3-1j",
      summary:"Running · 2 of 5 regimes complete · current run bear-q3-24." },
    { id:"v3.1.k", experiment:"prompt-tweak",       gate:"…",    deltaSharpe:null,  sharpe:null, status:"running",
      attestations:{endorse:0, question:0, reject:0}, kept:false, seed:"btc-momentum-7a91-v3-1k",
      summary:"Running · 1 of 5 regimes complete · current run bull-q1-25." },
    { id:"v3.1.l", experiment:"agent-add",          gate:"…",    deltaSharpe:null,  sharpe:null, status:"queued",
      attestations:{endorse:0, question:0, reject:0}, kept:false, seed:"btc-momentum-7a91-v3-1l",
      summary:"Queued · waiting on Stage-2 token bucket." },
    { id:"v3.1.m", experiment:"prompt-tweak",       gate:"…",    deltaSharpe:null,  sharpe:null, status:"queued",
      attestations:{endorse:0, question:0, reject:0}, kept:false, seed:"btc-momentum-7a91-v3-1m",
      summary:"Queued." },
    { id:"v3.1.n", experiment:"threshold-tune",     gate:"X",    deltaSharpe:null,  sharpe:null, status:"failed",
      attestations:{endorse:0, question:0, reject:0}, kept:false, seed:"btc-momentum-7a91-v3-1n",
      summary:"Failed · executor crashed mid-bar on flash-crash-24-08 · retried 2×." },
  ],
};

// Recent completed cycles for history table
const AR_RECENT_CYCLES = [
  { id:"cyc-01N6T1J3", lineage:"eth-mean-rev",   parent:"eth-mr-v3",        variants:12, kept:1, gate:8, when:"last night · 7h 14m",
    deltaTop:+0.18, tokens:"7.8M", costUSD:"$3.84" },
  { id:"cyc-01N5W4F9", lineage:"sol-strategist", parent:"sol-strategist-v4",variants:10, kept:3, gate:7, when:"2d ago · 6h 02m",
    deltaTop:+0.41, tokens:"6.4M", costUSD:"$3.12" },
  { id:"cyc-01N4P3A2", lineage:"btc-momentum",   parent:"btc-momentum-v2.1",variants:14, kept:2, gate:9, when:"3d ago · 6h 48m",
    deltaTop:+0.27, tokens:"8.9M", costUSD:"$4.41" },
  { id:"cyc-01N3K7B1", lineage:"multi-asset",    parent:"multi-asset-v1.4", variants:8,  kept:0, gate:3, when:"4d ago · 5h 22m",
    deltaTop:-0.04, tokens:"5.1M", costUSD:"$2.48" },
  { id:"cyc-01N2H1E0", lineage:"btc-grid",       parent:"btc-grid-v1.2",    variants:6,  kept:1, gate:4, when:"5d ago · 4h 02m",
    deltaTop:+0.13, tokens:"3.6M", costUSD:"$1.72" },
];

// =============================================================================
//  COMPONENTS
// =============================================================================

// Status pill — uses the project's StatusPill but adds AR-specific tones via wrapping.
// (operator-facing labels per terminology lock §1 + §11 — "KEPT" / "SUSPECT" / "DROPPED" replaces PASS/WARN/FAIL)
const ARStatusPill = ({ status, dense = false }) => {
  const map = {
    breeding:{ tone:"gold",    label:"RUNNING",  pulse:true  },
    cooled:  { tone:"neutral", label:"COOLED",   pulse:false },
    paused:  { tone:"warn",    label:"PAUSED",   pulse:false },
    running: { tone:"info",    label:"TESTING",  pulse:true  },
    done:    { tone:"gold",    label:"DONE",     pulse:false },
    queued:  { tone:"mute",    label:"QUEUED",   pulse:false },
    failed:  { tone:"danger",  label:"FAILED",   pulse:false },
    PASS:    { tone:"gold",    label:"KEPT",     pulse:false },
    WARN:    { tone:"warn",    label:"SUSPECT",  pulse:false },
    FAIL:    { tone:"danger",  label:"DROPPED",  pulse:false },
  };
  const c = map[status] || map.queued;
  return <StatusPill tone={c.tone} pulse={c.pulse}>{c.label}</StatusPill>;
};

// Experiment pill — small colored chip showing what kind of change a variant is.
const ExperimentPill = ({ kind, withLabel = true }) => {
  const m = AR_EXPERIMENTS[kind] || { label:kind, desc:"", tone:"neutral" };
  const tones = {
    info:    { fg:"var(--info)",   bd:"rgba(95,168,255,0.40)", bg:"rgba(95,168,255,0.10)" },
    violet:  { fg:"var(--violet)", bd:"rgba(167,139,250,0.40)", bg:"rgba(167,139,250,0.10)" },
    gold:    { fg:"var(--gold)",   bd:"var(--gold-soft)",      bg:"var(--gold-bg)" },
    warn:    { fg:"var(--warn)",   bd:"rgba(255,176,32,0.40)", bg:"rgba(255,176,32,0.08)" },
    neutral: { fg:"var(--text-2)", bd:"var(--border-strong)",  bg:"var(--surface-elev)" },
  };
  const t = tones[m.tone] || tones.neutral;
  return (
    <span style={{
      display:"inline-flex", alignItems:"center", gap:5,
      padding:"2px 6px", borderRadius:3,
      border:`1px solid ${t.bd}`, background:t.bg, color:t.fg,
      fontFamily:"'Geist Mono', monospace", fontSize:10, letterSpacing:"0.04em", fontWeight:500,
      whiteSpace:"nowrap",
    }}>
      <span style={{width:4, height:4, borderRadius:"50%", background:t.fg}}/>
      {withLabel && m.label}
    </span>
  );
};

// Regime kind icon — gives bull / bear / chop / shock a distinct glyph.
const RegimeIcon = ({ kind, size = 12, color = "currentColor" }) => {
  const paths = {
    bull:  <path d="M2 11l4-5 3 3 5-7" stroke={color} strokeWidth="1.5" fill="none" strokeLinecap="round" strokeLinejoin="round"/>,
    bear:  <path d="M2 3l4 5 3-3 5 7" stroke={color} strokeWidth="1.5" fill="none" strokeLinecap="round" strokeLinejoin="round"/>,
    chop:  <path d="M2 7h2l1.5-3 2 6 2-5 2 4 2-3h1" stroke={color} strokeWidth="1.5" fill="none" strokeLinecap="round" strokeLinejoin="round"/>,
    shock: <path d="M8 1l-3 7h3l-1 5 4-7H8l2-5z" stroke={color} strokeWidth="1.5" fill="none" strokeLinecap="round" strokeLinejoin="round"/>,
  };
  return (
    <svg width={size} height={size} viewBox="0 0 14 14" style={{display:"block", flexShrink:0}}>
      {paths[kind] || paths.chop}
    </svg>
  );
};

const REGIME_KIND_COLOR = {
  bull:  "var(--gold)",
  bear:  "var(--danger)",
  chop:  "var(--violet)",
  shock: "var(--warn)",
};

// Δ-Sharpe cell — shaded green if positive, red if negative; muted if null.
const DeltaSharpeCell = ({ value, size = "md" }) => {
  if (value === null || value === undefined) {
    return <span className="mono" style={{
      fontSize: size === "lg" ? 14 : 12, color:"var(--text-4)",
    }}>—</span>;
  }
  const positive = value >= 0;
  const sign = positive ? "+" : "";
  return (
    <span className="mono" style={{
      fontSize: size === "lg" ? 14 : 12, fontWeight:600,
      color: positive ? "var(--gold)" : "var(--danger)",
      letterSpacing:"-0.01em",
    }}>
      {sign}{value.toFixed(2)} Δ
    </span>
  );
};

// Tiny radial dial used in the "cycle in flight" header — a circular progress
// indicator with the percent number in the middle. ~64px.
const CycleDial = ({ progress = 0.4, size = 60, stroke = 5, label }) => {
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const dash = `${c * progress} ${c}`;
  return (
    <div style={{position:"relative", width:size, height:size, flexShrink:0}}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
        <circle cx={size/2} cy={size/2} r={r} fill="none"
          stroke="var(--border-strong)" strokeWidth={stroke}/>
        <circle cx={size/2} cy={size/2} r={r} fill="none"
          stroke="var(--gold)" strokeWidth={stroke}
          strokeDasharray={dash} strokeLinecap="round"
          transform={`rotate(-90 ${size/2} ${size/2})`}/>
      </svg>
      <div style={{
        position:"absolute", inset:0, display:"flex", alignItems:"center", justifyContent:"center",
        flexDirection:"column",
      }}>
        <span className="mono" style={{
          fontSize:14, fontWeight:600, color:"var(--gold)", lineHeight:1,
        }}>{Math.round(progress * 100)}%</span>
        {label && <span style={{
          fontSize:8.5, color:"var(--text-3)", letterSpacing:"0.14em",
          fontFamily:"'Geist Mono', monospace", marginTop:2,
        }}>{label}</span>}
      </div>
    </div>
  );
};

// Progress bar — used in many places.
const ARProgressBar = ({ value, total, color = "var(--gold)", height = 4 }) => (
  <div style={{
    width:"100%", height, borderRadius:height/2,
    background:"var(--surface-elev)", overflow:"hidden", position:"relative",
  }}>
    <div style={{
      width:`${(value/total) * 100}%`, height:"100%", background:color, borderRadius:height/2,
    }}/>
  </div>
);

// Gate verdict — operator-facing values per lock §3 ("KEPT" / "SUSPECT" / "DROPPED")
const GateBadge = ({ verdict, size = "md" }) => {
  const map = {
    PASS: { color:"var(--gold)",   bd:"var(--gold-soft)",      bg:"var(--gold-bg)",        icon:"check", label:"KEPT" },
    WARN: { color:"var(--warn)",   bd:"rgba(255,176,32,0.40)", bg:"rgba(255,176,32,0.10)", icon:"info" , label:"SUSPECT" },
    FAIL: { color:"var(--danger)", bd:"rgba(255,77,77,0.40)",  bg:"rgba(255,77,77,0.10)",  icon:"info" , label:"DROPPED" },
    "…":  { color:"var(--text-3)", bd:"var(--border-strong)",  bg:"var(--surface-elev)",   icon:null   , label:"…" },
    "X":  { color:"var(--text-4)", bd:"var(--border)",         bg:"transparent",           icon:null   , label:"—" },
  };
  const m = map[verdict] || map["…"];
  const padding = size === "sm" ? "2px 6px" : "3px 8px";
  const fs = size === "sm" ? 9 : 10;
  return (
    <span style={{
      display:"inline-flex", alignItems:"center", gap:5,
      padding, borderRadius:3,
      border:`1px solid ${m.bd}`, background:m.bg, color:m.color,
    }}>
      {m.icon && <Icon name={m.icon} size={fs} color={m.color} sw={2}/>}
      <span className="mono" style={{
        fontSize:fs, letterSpacing:"0.16em", fontWeight:700,
      }}>{m.label}</span>
    </span>
  );
};

Object.assign(window, {
  AR_REGIMES, AR_POPULATION, AR_EXPERIMENTS, AR_CURRENT_CYCLE, AR_RECENT_CYCLES,
  ARStatusPill, ExperimentPill, RegimeIcon, REGIME_KIND_COLOR, DeltaSharpeCell,
  CycleDial, ARProgressBar, GateBadge,
});
