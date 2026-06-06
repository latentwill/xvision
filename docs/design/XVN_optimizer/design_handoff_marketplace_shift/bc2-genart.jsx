// Deterministic gen-art for a strategy.
// Input: a string seed (lineage_id + variant_hash). Output: SVG content
// that fills a square box and looks consistent at 32px → 600px.
//
// Lineage-coherent: strategies that share a base seed prefix share a palette
// and composition family, so a lineage tree looks visually related.

// ── seeded PRNG ──
function bc2Hash(str) {
  let h = 2166136261;
  for (let i = 0; i < str.length; i++) {
    h ^= str.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}
function bc2Rng(seed) {
  let s = seed | 0;
  return () => {
    s = Math.imul(s ^ (s >>> 15), 2246822507);
    s = Math.imul(s ^ (s >>> 13), 3266489909);
    s = s ^ (s >>> 16);
    return ((s >>> 0) % 1_000_000) / 1_000_000;
  };
}

// ── curated palettes — one per lineage base ──
const BC2_PALETTES = [
  { bg:"#0B1A12", a:"#00E676", b:"#5FA8FF", c:"#F4E66B" }, // green/cyan
  { bg:"#0E1228", a:"#A78BFA", b:"#5FA8FF", c:"#F472B6" }, // violet/sky
  { bg:"#1A0F0B", a:"#FBBF24", b:"#FB923C", c:"#F87171" }, // sunset
  { bg:"#0B1612", a:"#34D399", b:"#22D3EE", c:"#A7F3D0" }, // mint
  { bg:"#1A1010", a:"#F472B6", b:"#FB7185", c:"#FCD34D" }, // hot pink
  { bg:"#0A0F1A", a:"#60A5FA", b:"#818CF8", c:"#E0E7FF" }, // cool blue
  { bg:"#161313", a:"#E879F9", b:"#A78BFA", c:"#67E8F9" }, // neon
  { bg:"#0E1611", a:"#84CC16", b:"#22D3EE", c:"#FACC15" }, // lime
];

// Composition families: stripes, rings, blob, mesh
const BC2_FAMILIES = ["mesh", "rings", "blob", "stripes"];

// ── main API: GenArt component (square, fills 100%) ──
const GenArt = ({ seed = "default", size = 80, decorative = true, style }) => {
  const h = bc2Hash(seed);
  const rng = bc2Rng(h);
  const palette = BC2_PALETTES[h % BC2_PALETTES.length];
  const family  = BC2_FAMILIES[(h >>> 4) % BC2_FAMILIES.length];
  const id = `g-${(h >>> 0).toString(36)}`;

  // Generate primitives once per render (deterministic from rng)
  const shapes = [];
  if (family === "rings") {
    const cx = 50 + (rng() - 0.5) * 30;
    const cy = 50 + (rng() - 0.5) * 30;
    const n  = 4 + Math.floor(rng() * 4);
    for (let i = 0; i < n; i++) {
      shapes.push({ kind:"ring", cx, cy, r: 10 + i * (60 / n),
        stroke: i % 2 === 0 ? palette.a : palette.b,
        opacity: 0.25 + 0.15 * (i % 3) });
    }
    shapes.push({ kind:"dot", cx, cy, r: 4 + rng() * 4, fill: palette.c });
  } else if (family === "blob") {
    const pts = [];
    const n = 7;
    for (let i = 0; i < n; i++) {
      const ang = (i / n) * Math.PI * 2;
      const r = 22 + rng() * 18;
      pts.push([ 50 + Math.cos(ang) * r, 50 + Math.sin(ang) * r ]);
    }
    shapes.push({ kind:"blob", pts, fill: palette.a, opacity: 0.85 });
    // accent dots
    for (let i = 0; i < 3; i++) {
      shapes.push({ kind:"dot", cx: rng() * 100, cy: rng() * 100,
        r: 1 + rng() * 2.5,
        fill: i === 0 ? palette.c : palette.b, opacity: 0.9 });
    }
  } else if (family === "stripes") {
    const n = 5 + Math.floor(rng() * 4);
    const angle = rng() * 90 - 45;
    for (let i = 0; i < n; i++) {
      shapes.push({ kind:"stripe", angle, y: (i / n) * 100 + rng() * 6,
        h: 2 + rng() * 6,
        fill: i % 2 === 0 ? palette.a : (i % 3 === 0 ? palette.c : palette.b),
        opacity: 0.55 + (i % 3) * 0.15 });
    }
  } else { // mesh
    const cols = 4 + Math.floor(rng() * 3);
    const rows = 4 + Math.floor(rng() * 3);
    for (let y = 0; y < rows; y++) {
      for (let x = 0; x < cols; x++) {
        const r = 1.4 + rng() * 4;
        const c = rng();
        shapes.push({ kind:"dot",
          cx: (x + 0.5) * (100/cols), cy: (y + 0.5) * (100/rows),
          r,
          fill: c < 0.6 ? palette.a : c < 0.9 ? palette.b : palette.c,
          opacity: 0.55 + rng() * 0.45 });
      }
    }
  }

  return (
    <svg
      viewBox="0 0 100 100"
      width={size} height={size}
      role={decorative ? "presentation" : undefined}
      style={{display:"block", borderRadius:4, ...style}}
      xmlns="http://www.w3.org/2000/svg"
    >
      <defs>
        <radialGradient id={`${id}-rg`} cx="50%" cy="35%" r="80%">
          <stop offset="0%"   stopColor={palette.a} stopOpacity="0.18"/>
          <stop offset="100%" stopColor={palette.bg} stopOpacity="1"/>
        </radialGradient>
      </defs>
      <rect x="0" y="0" width="100" height="100" fill={`url(#${id}-rg)`}/>
      {shapes.map((s, i) => {
        if (s.kind === "ring") return (
          <circle key={i} cx={s.cx} cy={s.cy} r={s.r}
            fill="none" stroke={s.stroke} strokeWidth="1.2" opacity={s.opacity}/>
        );
        if (s.kind === "dot") return (
          <circle key={i} cx={s.cx} cy={s.cy} r={s.r}
            fill={s.fill} opacity={s.opacity}/>
        );
        if (s.kind === "blob") {
          const d = s.pts.map((p, j) => `${j === 0 ? "M" : "L"} ${p[0]} ${p[1]}`).join(" ") + " Z";
          return <path key={i} d={d} fill={s.fill} opacity={s.opacity}/>;
        }
        if (s.kind === "stripe") return (
          <rect key={i} x="-20" y={s.y} width="140" height={s.h}
            fill={s.fill} opacity={s.opacity}
            transform={`rotate(${s.angle} 50 50)`}/>
        );
        return null;
      })}
    </svg>
  );
};

// Sparkline — 30 points, mostly upward trend if positive return, else jagged down.
const Sparkline = ({ seed = "x", positive = true, width = 88, height = 24, color }) => {
  const rng = bc2Rng(bc2Hash(seed));
  const pts = [];
  let v = 50;
  for (let i = 0; i < 30; i++) {
    const drift = positive ? 0.6 : -0.4;
    v += drift + (rng() - 0.5) * 6;
    v = Math.max(8, Math.min(92, v));
    pts.push(v);
  }
  // Map to coords
  const xs = pts.map((_, i) => (i / (pts.length - 1)) * width);
  const ys = pts.map((p) => height - (p / 100) * height);
  const d = xs.map((x, i) => `${i === 0 ? "M" : "L"} ${x.toFixed(2)} ${ys[i].toFixed(2)}`).join(" ");
  const fill = positive ? "var(--gold)" : "var(--danger)";
  const stroke = color || fill;
  return (
    <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`}
      style={{display:"block"}}>
      <path d={d} fill="none" stroke={stroke} strokeWidth="1.3"
        strokeLinejoin="round" strokeLinecap="round"/>
    </svg>
  );
};

// Small 🤖-replacement icon (operator-style, no emoji)
const AgentIcon = ({ size = 11, color = "var(--gold)" }) => (
  <svg width={size} height={size} viewBox="0 0 12 12" fill="none"
    stroke={color} strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round"
    style={{display:"block", flexShrink:0}}>
    <rect x="2" y="3" width="8" height="6.5" rx="1.5"/>
    <circle cx="4.5" cy="6.2" r="0.6" fill={color}/>
    <circle cx="7.5" cy="6.2" r="0.6" fill={color}/>
    <path d="M6 1.5v1.5"/>
    <circle cx="6" cy="1.1" r="0.4" fill={color}/>
  </svg>
);

Object.assign(window, { GenArt, Sparkline, AgentIcon, bc2Hash });
