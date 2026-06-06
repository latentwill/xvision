/* xvn — shared uPlot helpers, draw plugins, KlineCharts theme */

// ────────────────────────────────────────────────────────────────────
// uPlot base theme
// ────────────────────────────────────────────────────────────────────

const FONT_MONO = '11px "Geist Mono", monospace';
const FONT_MONO_SM = '10px "Geist Mono", monospace';
const PAL = window.XVN_PALETTE;

window.xvnUplotTheme = {
  axes: {
    stroke: PAL.mute,
    grid:   { stroke: 'rgba(255, 255, 255, 0.04)', width: 1 },
    ticks:  { stroke: 'rgba(255, 255, 255, 0.08)', width: 1, size: 4 },
    font:   FONT_MONO_SM,
    labelFont: FONT_MONO,
  },
  cursor: {
    points: { size: 7, fill: (u, sIdx) => u.series[sIdx].stroke() },
  },
};

// build an axes config that lays out cleanly inside xvn
window.xvnAxes = function(opts = {}){
  return [
    {
      stroke: PAL.mute,
      grid: { stroke: 'rgba(255, 255, 255, 0.04)', width: 1 },
      ticks: { show: false },
      font: FONT_MONO_SM,
      gap: 6,
      size: 28,
      values: opts.timeValues || ((u, vals) => vals.map(v => {
        const d = new Date(v * 1000);
        return d.toLocaleString('en-US', { month: 'short' }) + " '" + String(d.getFullYear()).slice(2);
      })),
    },
    {
      stroke: PAL.mute,
      grid: { stroke: 'rgba(255, 255, 255, 0.04)', width: 1, dash: [2,4] },
      ticks: { show: false },
      font: FONT_MONO_SM,
      gap: 6,
      size: opts.ySize || 46,
      values: opts.yValues || ((u, vals) => vals.map(v => (v>=0?'+':'') + v.toFixed(0) + '%')),
    },
  ];
};

// scale/axis for index-based (no time)
window.xvnAxesIdx = function(opts = {}){
  return [
    { stroke: PAL.mute, grid: { show:false }, ticks: { show:false }, font: FONT_MONO_SM, size: 22, values: () => [] },
    { stroke: PAL.mute, grid: { stroke: 'rgba(255, 255, 255, 0.04)', width:1, dash:[2,4] }, ticks: { show:false }, font: FONT_MONO_SM, size: opts.ySize || 40,
      values: opts.yValues || ((u, vals) => vals.map(v => v.toFixed(0)+'%')) },
  ];
};

// build a single line series spec
window.xvnLine = function(label, color, opts={}){
  const dash = opts.dashed ? [4,4] : null;
  return {
    label,
    stroke: color,
    width: opts.width ?? 1.4,
    points: { show: false },
    dash,
    ...opts.extra,
  };
};

// area-fill draw plugin — gradient under curve, scoped to one series
window.xvnAreaFill = function(seriesIdx, topColor, bottomAlpha = 0){
  return {
    hooks: {
      draw: u => {
        const ctx = u.ctx;
        const s = u.series[seriesIdx];
        if (!s.show) return;
        const xData = u.data[0];
        const yData = u.data[seriesIdx];
        if (!yData) return;
        const left = u.bbox.left, top = u.bbox.top, width = u.bbox.width, height = u.bbox.height;
        ctx.save();
        ctx.beginPath();
        let started = false;
        for (let i=0;i<xData.length;i++){
          const x = u.valToPos(xData[i], 'x', true);
          const y = u.valToPos(yData[i], s.scale || 'y', true);
          if (y == null || !isFinite(y)) continue;
          if (!started){ ctx.moveTo(x, y); started = true; } else ctx.lineTo(x, y);
        }
        const lastX = u.valToPos(xData[xData.length-1], 'x', true);
        const firstX = u.valToPos(xData[0], 'x', true);
        const zeroY = u.valToPos(0, s.scale || 'y', true);
        ctx.lineTo(lastX, zeroY);
        ctx.lineTo(firstX, zeroY);
        ctx.closePath();
        const grad = ctx.createLinearGradient(0, top, 0, top + height);
        grad.addColorStop(0, topColor);
        grad.addColorStop(1, `rgba(0,0,0,${bottomAlpha})`);
        ctx.fillStyle = grad;
        ctx.fill();
        ctx.restore();
      },
    },
  };
};

// gold halo dot on the last data point of a series
window.xvnLastDot = function(seriesIdx, color, label){
  return {
    hooks: {
      draw: u => {
        const ctx = u.ctx;
        const xData = u.data[0];
        const yData = u.data[seriesIdx];
        if (!xData || !yData) return;
        const i = xData.length - 1;
        const x = u.valToPos(xData[i], 'x', true);
        const y = u.valToPos(yData[i], 'y', true);
        ctx.save();
        // halo
        ctx.beginPath();
        ctx.arc(x, y, 7, 0, Math.PI*2);
        ctx.fillStyle = color + '22';
        ctx.fill();
        ctx.beginPath();
        ctx.arc(x, y, 3.2, 0, Math.PI*2);
        ctx.fillStyle = color;
        ctx.fill();
        ctx.strokeStyle = '#000000';
        ctx.lineWidth = 1.2;
        ctx.stroke();
        ctx.restore();
      },
    },
  };
};

// vertical regime bands (array of {x0, x1, fill, label})
window.xvnRegimeBands = function(bands){
  return {
    hooks: {
      drawClear: u => {
        const ctx = u.ctx;
        ctx.save();
        for (const b of bands){
          const x0 = u.valToPos(b.x0, 'x', true);
          const x1 = u.valToPos(b.x1, 'x', true);
          ctx.fillStyle = b.fill;
          ctx.fillRect(x0, u.bbox.top, x1 - x0, u.bbox.height);
        }
        ctx.restore();
      },
    },
  };
};

// ────────────────────────────────────────────────────────────────────
// React hook — mount uPlot to a ref, rebuild when deps change.
// `buildOpts(parent)` returns an opts object that INCLUDES a `data` field
// (we pass it as the 2nd arg to uPlot for you). ResizeObserver re-mounts
// on width changes so charts work even when the design canvas hands us a
// container that starts 0×0.
// ────────────────────────────────────────────────────────────────────

window.useUplot = function(buildOpts, deps = []){
  const ref = React.useRef(null);
  const plotRef = React.useRef(null);
  const lastWidth = React.useRef(0);
  React.useEffect(() => {
    if (!ref.current) return;
    const tryMount = () => {
      if (!ref.current) return;
      const w = ref.current.clientWidth;
      if (w < 10) return false;
      const opts = buildOpts(ref.current);
      const data = opts.data;
      delete opts.data;
      if (plotRef.current){ plotRef.current.destroy(); plotRef.current = null; }
      plotRef.current = new uPlot(opts, data, ref.current);
      lastWidth.current = w;
      return true;
    };
    tryMount();
    const ro = new ResizeObserver(entries => {
      for (const e of entries){
        const w = e.contentRect.width;
        if (w < 10) continue;
        if (Math.abs(w - lastWidth.current) > 2){
          tryMount();
        }
      }
    });
    ro.observe(ref.current);
    return () => {
      ro.disconnect();
      if (plotRef.current){ plotRef.current.destroy(); plotRef.current = null; }
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return ref;
};

// ────────────────────────────────────────────────────────────────────
// KlineCharts custom theme
// ────────────────────────────────────────────────────────────────────

window.xvnKLineTheme = {
  grid: {
    show: true,
    horizontal: { color: 'rgba(255, 255, 255, 0.04)', size: 1, style: 'dashed', dashedValue:[2,4] },
    vertical:   { color: 'rgba(255, 255, 255, 0.04)', size: 1, style: 'dashed', dashedValue:[2,4] },
  },
  candle: {
    type: 'candle_solid',
    bar: {
      // green/red — actual market semantics. Tinted toward xvn's warm
      // palette (slightly muted, with gold-leaning green) so they still
      // feel like part of the family.
      upColor: '#3FAE6B',        downColor: '#FF4D4D',
      noChangeColor: PAL.mute,
      upBorderColor: '#3FAE6B',  downBorderColor: '#FF4D4D',
      upWickColor: '#3FAE6B',    downWickColor: '#FF4D4D',
    },
    tooltip: { showRule: 'none' },
    priceMark: {
      show: true,
      high: { color: PAL.mute, textFamily: 'Geist Mono', textSize: 10 },
      low:  { color: PAL.mute, textFamily: 'Geist Mono', textSize: 10 },
      last: {
        upColor: '#3FAE6B', downColor: '#FF4D4D', noChangeColor: PAL.mute,
        line: { show: true, style: 'dashed', dashedValue:[3,3], size: 1 },
        text: { show: true, color: '#000000', size: 10, family: 'Geist Mono', paddingLeft:4,paddingTop:2,paddingRight:4,paddingBottom:2, borderSize:0, borderColor:'transparent', borderRadius:2 },
      },
    },
  },
  xAxis: {
    axisLine: { color: 'rgba(255, 255, 255, 0.08)', size: 1 },
    tickLine: { show: false },
    tickText: { color: PAL.mute, family: 'Geist Mono', size: 10 },
  },
  yAxis: {
    position: 'right',
    axisLine: { color: 'rgba(255, 255, 255, 0.08)', size: 1 },
    tickLine: { show: false },
    tickText: { color: PAL.mute, family: 'Geist Mono', size: 10 },
  },
  crosshair: {
    horizontal: {
      line: { show: true, color: 'rgba(255, 255, 255, 0.25)', style: 'dashed', dashedValue:[3,3], size: 1 },
      text: { color: PAL.cream, backgroundColor: '#0E0E0E', family: 'Geist Mono', size: 10, paddingLeft:6,paddingRight:6,paddingTop:3,paddingBottom:3, borderColor:'#2A2A2A', borderSize:1 },
    },
    vertical: {
      line: { show: true, color: 'rgba(255, 255, 255, 0.25)', style: 'dashed', dashedValue:[3,3], size: 1 },
      text: { color: PAL.cream, backgroundColor: '#0E0E0E', family: 'Geist Mono', size: 10, paddingLeft:6,paddingRight:6,paddingTop:3,paddingBottom:3, borderColor:'#2A2A2A', borderSize:1 },
    },
  },
  indicator: {
    tooltip: { showRule:'none' },
  },
};
