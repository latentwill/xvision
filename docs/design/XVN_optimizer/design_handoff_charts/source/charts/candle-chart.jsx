/* xvn — custom canvas candle chart component
   Pure canvas. No external dep. Renders OHLC + volume + EMA + crosshair + price/time axes.
   Exposes ref methods: xForIndex(idx), yForPrice(p), getBounds()
*/

window.XvnCandleChart = React.forwardRef(function XvnCandleChart(props, fwdRef){
  const {
    candles, ema = 21, padding = { l: 0, r: 64, t: 16, b: 28 },
    volumeRatio = 0.18, lastPriceTag = true,
    onLayout, // (api) => void; called whenever layout recomputes
  } = props;
  const rootRef = React.useRef(null);
  const canvasRef = React.useRef(null);
  const [hover, setHover] = React.useState(null); // { i, x, y, price }
  const [size, setSize] = React.useState({ w: 0, h: 0, dpr: 1 });
  const [layoutTick, setLayoutTick] = React.useState(0);

  // ── ema ──
  const emaSeries = React.useMemo(() => {
    if (!ema) return null;
    const k = 2 / (ema + 1);
    const out = new Array(candles.length);
    let v = candles[0].close;
    for (let i = 0; i < candles.length; i++){
      v = candles[i].close * k + v * (1 - k);
      out[i] = v;
    }
    return out;
  }, [candles, ema]);

  // ── layout ──
  const layout = React.useMemo(() => {
    const { w, h } = size;
    if (w < 20 || h < 20) return null;
    const inner = {
      x: padding.l,
      y: padding.t,
      w: w - padding.l - padding.r,
      h: h - padding.t - padding.b,
    };
    const volH = Math.max(36, Math.floor(inner.h * volumeRatio));
    const priceH = inner.h - volH - 6;
    const priceArea = { x: inner.x, y: inner.y, w: inner.w, h: priceH };
    const volArea = { x: inner.x, y: inner.y + priceH + 6, w: inner.w, h: volH };
    // price range
    let lo = Infinity, hi = -Infinity;
    for (const c of candles){
      if (c.low < lo) lo = c.low;
      if (c.high > hi) hi = c.high;
    }
    // pad price domain
    const pad = (hi - lo) * 0.04;
    lo -= pad; hi += pad;
    const volMax = candles.reduce((m, c) => Math.max(m, c.volume), 0) * 1.1;
    return { inner, priceArea, volArea, lo, hi, volMax };
  }, [size, candles, padding.l, padding.r, padding.t, padding.b, volumeRatio]);

  // ── coordinate functions ──
  const api = React.useMemo(() => {
    if (!layout) return null;
    const { priceArea, lo, hi } = layout;
    const n = candles.length;
    const slot = priceArea.w / n;
    return {
      xForIndex: (i) => priceArea.x + (i + 0.5) * slot,
      yForPrice: (p) => priceArea.y + (1 - (p - lo) / (hi - lo)) * priceArea.h,
      slotWidth: slot,
      priceArea,
      indexForX: (x) => Math.max(0, Math.min(n - 1, Math.floor((x - priceArea.x) / slot))),
      bounds: { w: size.w, h: size.h },
      layout,
    };
  }, [layout, candles.length, size.w, size.h]);

  React.useImperativeHandle(fwdRef, () => api, [api]);

  React.useEffect(() => {
    if (api && onLayout) onLayout(api);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [api]);

  // ── resize observer ──
  React.useEffect(() => {
    if (!rootRef.current) return;
    const update = () => {
      const el = rootRef.current;
      if (!el) return;
      // Use clientWidth/clientHeight (CSS pixels) — NOT getBoundingClientRect
      // which would include any design-canvas zoom transforms.
      const w = el.clientWidth;
      const h = el.clientHeight;
      const dpr = Math.max(1, window.devicePixelRatio || 1);
      if (w !== size.w || h !== size.h || dpr !== size.dpr){
        setSize({ w, h, dpr });
      }
    };
    update();
    // Two extra ticks after mount in case parent is still laying out
    const t1 = setTimeout(update, 80);
    const t2 = setTimeout(update, 320);
    const ro = new ResizeObserver(update);
    ro.observe(rootRef.current);
    return () => { clearTimeout(t1); clearTimeout(t2); ro.disconnect(); };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── draw ──
  React.useEffect(() => {
    if (!canvasRef.current || !layout) return;
    const cnv = canvasRef.current;
    const ctx = cnv.getContext('2d');
    const { w, h, dpr } = size;
    cnv.width  = Math.round(w * dpr);
    cnv.height = Math.round(h * dpr);
    cnv.style.width  = w + 'px';
    cnv.style.height = h + 'px';
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    drawAll(ctx, { candles, emaSeries, layout, w, h, hover, lastPriceTag, padding });
  }, [size, candles, emaSeries, layout, hover, lastPriceTag, padding]);

  // ── mouse → crosshair ──
  function onMove(e){
    if (!api) return;
    const r = canvasRef.current.getBoundingClientRect();
    const x = e.clientX - r.left;
    const y = e.clientY - r.top;
    if (x < layout.priceArea.x || x > layout.priceArea.x + layout.priceArea.w ||
        y < layout.priceArea.y || y > layout.priceArea.y + layout.priceArea.h + layout.volArea.h + 6) {
      setHover(null); return;
    }
    const i = api.indexForX(x);
    setHover({ i, x: api.xForIndex(i), y, candle: candles[i] });
  }
  function onLeave(){ setHover(null); }

  return (
    <div ref={rootRef} style={{position:'absolute', inset:0, width:'100%', height:'100%'}}>
      <canvas ref={canvasRef} onMouseMove={onMove} onMouseLeave={onLeave}
        style={{position:'absolute', inset:0, display:'block'}}/>
    </div>
  );
});

function drawAll(ctx, S){
  const { candles, emaSeries, layout, w, h, hover, lastPriceTag, padding } = S;
  const { priceArea, volArea, lo, hi, volMax } = layout;
  // ── bg already from container ──
  // ── grid ──
  ctx.save();
  ctx.strokeStyle = 'rgba(255, 255, 255, 0.05)';
  ctx.lineWidth = 1;
  // horizontal price grid: 5 lines
  const nLines = 5;
  ctx.setLineDash([2, 4]);
  for (let i=0;i<=nLines;i++){
    const p = lo + (hi - lo) * (i / nLines);
    const y = priceArea.y + (1 - (p - lo) / (hi - lo)) * priceArea.h;
    ctx.beginPath();
    ctx.moveTo(priceArea.x, y);
    ctx.lineTo(priceArea.x + priceArea.w, y);
    ctx.stroke();
    // tick label on right
    ctx.fillStyle = '#5F6670';
    ctx.font = '10px JetBrains Mono, monospace';
    ctx.textAlign = 'left';
    ctx.textBaseline = 'middle';
    ctx.setLineDash([]);
    ctx.fillText(p.toFixed(0), priceArea.x + priceArea.w + 8, y);
    ctx.setLineDash([2,4]);
  }
  ctx.setLineDash([]);
  ctx.restore();

  // ── x-axis ticks (sparse) ──
  ctx.save();
  ctx.fillStyle = '#5F6670';
  ctx.font = '10px JetBrains Mono, monospace';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'top';
  const tickEvery = Math.max(1, Math.floor(candles.length / 8));
  for (let i = tickEvery; i < candles.length; i += tickEvery){
    const x = priceArea.x + (i + 0.5) * (priceArea.w / candles.length);
    const t = candles[i].timestamp || (candles[i].time * 1000);
    const d = new Date(t);
    const label = d.toLocaleDateString('en-US', { month:'short', day:'numeric' });
    ctx.fillText(label, x, volArea.y + volArea.h + 6);
  }
  ctx.restore();

  // ── volume bars ──
  ctx.save();
  const slot = priceArea.w / candles.length;
  for (let i=0;i<candles.length;i++){
    const c = candles[i];
    const isUp = c.close >= c.open;
    const x = priceArea.x + i * slot;
    const bw = Math.max(1, slot - 1);
    const vH = (c.volume / volMax) * volArea.h;
    ctx.fillStyle = isUp ? 'rgba(63,174,107,0.42)' : 'rgba(255, 77, 77, 0.42)';
    ctx.fillRect(x + 0.5, volArea.y + volArea.h - vH, bw, vH);
  }
  ctx.restore();

  // ── candles ──
  ctx.save();
  const yForPrice = (p) => priceArea.y + (1 - (p - lo) / (hi - lo)) * priceArea.h;
  for (let i=0;i<candles.length;i++){
    const c = candles[i];
    const isUp = c.close >= c.open;
    const color = isUp ? '#3FAE6B' : '#FF4D4D';
    const cx = priceArea.x + (i + 0.5) * slot;
    const yO = yForPrice(c.open);
    const yC = yForPrice(c.close);
    const yH = yForPrice(c.high);
    const yL = yForPrice(c.low);
    const bw = Math.max(2, Math.min(slot * 0.7, 8));
    // wick
    ctx.strokeStyle = color;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(cx + 0.5, yH);
    ctx.lineTo(cx + 0.5, yL);
    ctx.stroke();
    // body
    ctx.fillStyle = color;
    const bodyY = Math.min(yO, yC);
    const bodyH = Math.max(1, Math.abs(yC - yO));
    ctx.fillRect(cx - bw/2, bodyY, bw, bodyH);
  }
  ctx.restore();

  // ── EMA ──
  if (emaSeries){
    ctx.save();
    ctx.strokeStyle = '#00E676';
    ctx.lineWidth = 1.2;
    ctx.beginPath();
    for (let i=0;i<candles.length;i++){
      const x = priceArea.x + (i + 0.5) * slot;
      const y = yForPrice(emaSeries[i]);
      if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
    }
    ctx.stroke();
    ctx.restore();
  }

  // ── last price tag ──
  if (lastPriceTag){
    const c = candles[candles.length - 1];
    const y = yForPrice(c.close);
    const isUp = c.close >= c.open;
    const color = isUp ? '#3FAE6B' : '#FF4D4D';
    ctx.save();
    ctx.setLineDash([3, 3]);
    ctx.strokeStyle = color + 'aa';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(priceArea.x, y);
    ctx.lineTo(priceArea.x + priceArea.w, y);
    ctx.stroke();
    ctx.setLineDash([]);
    // tag
    const label = c.close.toFixed(0);
    ctx.font = '10px JetBrains Mono, monospace';
    const tw = ctx.measureText(label).width;
    ctx.fillStyle = color;
    ctx.fillRect(priceArea.x + priceArea.w + 4, y - 8, tw + 10, 16);
    ctx.fillStyle = '#000000';
    ctx.textAlign = 'left';
    ctx.textBaseline = 'middle';
    ctx.fillText(label, priceArea.x + priceArea.w + 9, y);
    ctx.restore();
  }

  // ── crosshair ──
  if (hover){
    ctx.save();
    ctx.setLineDash([3, 3]);
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.32)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(hover.x, priceArea.y);
    ctx.lineTo(hover.x, volArea.y + volArea.h);
    ctx.moveTo(priceArea.x, hover.y);
    ctx.lineTo(priceArea.x + priceArea.w, hover.y);
    ctx.stroke();
    ctx.setLineDash([]);
    // ohlc label
    const c = hover.candle;
    const labels = [
      ['O', c.open.toFixed(1)],
      ['H', c.high.toFixed(1)],
      ['L', c.low.toFixed(1)],
      ['C', c.close.toFixed(1)],
    ];
    ctx.font = '10px JetBrains Mono, monospace';
    ctx.textBaseline = 'middle';
    ctx.fillStyle = 'rgba(10, 10, 10, 0.94)';
    ctx.strokeStyle = 'rgba(42, 42, 42, 0.7)';
    const bw = 160, bh = 16, bx = priceArea.x + 8, by = priceArea.y + 6;
    ctx.fillRect(bx, by, bw, bh);
    ctx.strokeRect(bx + 0.5, by + 0.5, bw - 1, bh - 1);
    ctx.textAlign = 'left';
    let xc = bx + 8;
    for (const [k, v] of labels){
      ctx.fillStyle = '#5F6670';
      ctx.fillText(k, xc, by + bh/2);
      ctx.fillStyle = '#FFFFFF';
      ctx.fillText(v, xc + 12, by + bh/2);
      xc += 38;
    }
    ctx.restore();
  }
}
