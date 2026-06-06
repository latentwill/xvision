/* Chart 3 — AI Annotation Chart (chart-only frame, 1200×680)
   KlineCharts candle pane. Callouts are positioned to real candle x via
   convertToPixel and clamped to the chart pane (no overlap with insight log).
   Insight log is collapsible. As the user pans the chart (KlineCharts native
   drag), callouts move with their candles and clip at chart edges.
*/

window.ChartAIAnnotation = function ChartAIAnnotation(){
  const PAL = window.XVN_PALETTE;
  const chartApiRef = React.useRef(null);
  const candles = React.useMemo(() => window.makeCandles(170, 63500, 17), []);
  const [recompute, setRecompute] = React.useState(0);
  const [logOpen, setLogOpen] = React.useState(true);

  // annotations anchored to candle indices.
  const ANNOTATIONS = React.useMemo(() => ([
    { idx:  22, side:'top',    type:'PATTERN',    title:'Bull Flag',         body:'Flag consolidation after impulse. Breakout > 64,920 likely retests 63,100 wick.',                  conf: 0.74, action:'WATCH' },
    { idx:  52, side:'bottom', type:'FLOW',       title:'Volume Divergence', body:'LL price with HH buy volume — accumulation footprint, 3-bar window.',                          conf: 0.68, action:'LONG' },
    { idx:  80, side:'top',    type:'RISK',       title:'Liquidation Wall',  body:'$48M long liq cluster at 65,800. Likely magnet on next vol expansion.',                         conf: 0.82, action:'CAUTION', danger:true },
    { idx: 110, side:'bottom', type:'REVERSION',  title:'RSI Reset',         body:'RSI cooled 71 → 47 without breaking trend. Mean-reversion re-entry zone.',                      conf: 0.61, action:'LONG' },
    { idx: 144, side:'top',    type:'STRUCTURE',  title:'Break of Structure',body:'HL → HH → BoS sequence confirmed. Bias flips bullish on close > 65,200.',                       conf: 0.79, action:'LONG' },
  ]), []);

  // ── compute callout positions from chart api ──
  // Spread top-row callouts evenly across the chart top so they never
  // overlap; spread bottom-row callouts evenly along the bottom.
  // Connector lines still go to the real candle anchor.
  const callouts = React.useMemo(() => {
    const api = chartApiRef.current;
    if (!api) return [];
    const cw  = api.bounds.w;
    const chh = api.bounds.h;
    const CW  = 210;
    // Reserve room: 12px left edge, 80px right (price axis labels)
    const usableW = cw - 12 - 80;
    const tops = ANNOTATIONS.filter(a => a.side === 'top');
    const bots = ANNOTATIONS.filter(a => a.side === 'bottom');
    function spread(items, rowY){
      const n = items.length;
      if (!n) return [];
      // available slot per item
      const slot = (usableW - CW) / Math.max(1, n - 1);
      return items.map((a, i) => {
        const cx = 12 + i * slot;
        const candle = candles[a.idx];
        const ax = api.xForIndex(a.idx);
        const ay = api.yForPrice(a.side === 'top' ? candle.high : candle.low);
        return { a, ax, ay, cx, cy: rowY, width: CW };
      });
    }
    return [
      ...spread(tops, 24),
      ...spread(bots, chh - 180),
    ];
  }, [recompute, ANNOTATIONS, candles]);

  return (
    <div data-screen-label="03 AI Annotation Chart" style={{
      width: 1200, height: 680,
      background:'var(--bg)', color:'var(--text)',
      border:'1px solid var(--border)', borderRadius: 8,
      display:'flex', flexDirection:'column', overflow:'hidden',
      position:'relative',
    }}>

      <style>{`
        @keyframes aiPulse {
          0%   { transform: scale(1);   opacity: 0.7; }
          100% { transform: scale(3.4); opacity: 0; }
        }
      `}</style>

      {/* header */}
      <div style={{
        display:'flex', alignItems:'center', justifyContent:'space-between',
        padding:'14px 18px', borderBottom:'1px solid var(--border-soft)',
        background:'var(--surface-card)', flex:'0 0 auto',
      }}>
        <div style={{display:'flex', alignItems:'center', gap:14}}>
          <div className="serif-i" style={{fontSize:22, letterSpacing:'-0.02em'}}>xvn</div>
          <div className="divider-v"></div>
          <div>
            <div className="caps" style={{marginBottom:2}}>BTCUSDT · 1h · Binance Perpetual</div>
            <div style={{display:'flex', alignItems:'center', gap:8}}>
              <span className="serif" style={{fontSize:20, color:'var(--text)'}}>65,128.40</span>
              <span className="mono" style={{fontSize:12, color:'#3FAE6B'}}>+1.84%</span>
              <span className="caps">24h</span>
            </div>
          </div>
        </div>
        <div style={{display:'flex', gap:10, alignItems:'center'}}>
          <span className="pill gold">
            <span style={{position:'relative', display:'inline-block', width:6, height:6}}>
              <span style={{position:'absolute', inset:0, borderRadius:'50%', background:'var(--gold)', animation:'aiPulse 1.8s ease-out infinite'}}></span>
              <span style={{position:'absolute', inset:0, borderRadius:'50%', background:'var(--gold)'}}></span>
            </span>
            AI Engine · live
          </span>
          <span className="pill ghost">model · xvn-annot-v3</span>
          <div className="toggle-row">
            {['Patterns','Risk','Flow','All'].map(t => <button key={t} className={t==='All'?'active':''}>{t}</button>)}
          </div>
        </div>
      </div>

      {/* body */}
      <div style={{display:'grid', gridTemplateColumns: logOpen ? '1fr 280px' : '1fr 36px', flex:1, minHeight:0, transition:'grid-template-columns 200ms ease'}}>

        {/* chart pane: positioned + overflow:hidden so callouts clip */}
        <div style={{position:'relative', overflow:'hidden', borderRight:'1px solid var(--border-soft)'}}>
          <XvnCandleChart
            candles={candles}
            ema={21}
            onLayout={(api) => { chartApiRef.current = api; setRecompute(r => r+1); }}
          />

          {/* callout overlay */}
          <div style={{position:'absolute', inset:0, pointerEvents:'none', zIndex:3}}>
            {/* connector lines */}
            <svg style={{position:'absolute', inset:0, width:'100%', height:'100%'}} preserveAspectRatio="none">
              {callouts.map((c, i) => {
                const CALLOUT_H_APPROX = 110;
                // line attaches from the callout edge to the candle anchor.
                // pick the corner of the callout closest to the anchor.
                const cxMid = c.cx + c.width / 2;
                const startX = c.ax > cxMid ? c.cx + c.width : c.cx;
                const startY = c.a.side === 'top' ? c.cy + CALLOUT_H_APPROX - 6 : c.cy + 6;
                return (
                  <g key={i}>
                    <line
                      x1={startX} y1={startY}
                      x2={c.ax}   y2={c.ay}
                      stroke={c.a.danger ? 'rgba(255, 77, 77, 0.65)' : 'rgba(0, 230, 118, 0.65)'}
                      strokeWidth="1"
                      strokeDasharray="3 3"/>
                    <circle cx={c.ax} cy={c.ay} r="6" fill="none"
                            stroke={c.a.danger ? 'rgba(255, 77, 77, 0.55)' : 'rgba(0, 230, 118, 0.55)'}
                            strokeWidth="1"/>
                    <circle cx={c.ax} cy={c.ay} r="2.4"
                            fill={c.a.danger ? '#FF4D4D' : '#00E676'}/>
                  </g>
                );
              })}
            </svg>

            {/* callout cards */}
            {callouts.map((c, i) => (
              <div key={i} className="callout" style={{
                left: c.cx, top: c.cy,
                width: 210,
                borderColor: c.a.danger ? 'rgba(255, 77, 77, 0.4)' : 'rgba(0, 230, 118, 0.32)',
              }}>
                <div className="callout-head" style={{color: c.a.danger ? '#FF4D4D' : '#00E676'}}>
                  <span>{c.a.type}</span>
                  <span style={{fontFamily:'JetBrains Mono, monospace', fontSize:9.5}}>conf {(c.a.conf*100).toFixed(0)}%</span>
                </div>
                <div className="serif" style={{fontSize:14, color:'var(--text)', marginBottom:3, letterSpacing:'-0.005em'}}>{c.a.title}</div>
                <div className="callout-body">{c.a.body}</div>
                <div className="callout-foot">
                  <span>idx · {c.a.idx}</span>
                  <span style={{color: c.a.danger ? '#FF4D4D' : '#00E676'}}>▸ {c.a.action}</span>
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* insight log — collapsible */}
        {logOpen ? (
          <div style={{display:'flex', flexDirection:'column', background:'var(--surface-card)', overflow:'hidden'}}>
            <div style={{
  display:'flex', alignItems:'center', justifyContent:'space-between',
  padding:'12px 14px 10px', borderBottom:'1px solid var(--border-soft)', flex:'0 0 auto',
}}>
            <div style={{display:'flex', alignItems:'center', gap:10}}>
              <h2 style={{margin:0, fontFamily:'Geist, sans-serif', fontWeight:500, fontSize:17, letterSpacing:'-0.01em', whiteSpace:'nowrap'}}>Insight Log</h2>
              <span className="caps">last 6h</span>
            </div>
              <button onClick={() => setLogOpen(false)} title="Collapse"
                style={{background:'transparent', border:'1px solid var(--border)', color:'var(--text-3)', padding:'2px 7px', borderRadius:3, cursor:'pointer', fontSize:13, lineHeight:1}}>›</button>
            </div>
            <div style={{padding:'10px 14px 14px', display:'flex', flexDirection:'column', gap:8, overflowY:'auto'}}>
              {[
                {t:'08:42', title:'Break of Structure', body:'HL→HH→BoS confirmed on 1h. Bias flips bullish.', tag:'STRUCTURE', conf:0.79},
                {t:'07:55', title:'RSI Reset', body:'RSI 71 → 47 without trend break. Mean-rev re-entry.', tag:'REVERSION', conf:0.61},
                {t:'06:18', title:'Liquidation Wall', body:'$48M longs at 65,800 — magnet on next vol up.', tag:'RISK', conf:0.82, danger:true},
                {t:'04:24', title:'Volume Divergence', body:'LL price, HH buy vol. Accumulation footprint.', tag:'FLOW', conf:0.68},
                {t:'02:10', title:'Bull Flag', body:'Flag after impulse. Break > 64,920 → 63,100 retest.', tag:'PATTERN', conf:0.74},
                {t:'00:32', title:'Funding Shift', body:'Funding flipped negative on Bybit; squeeze risk.', tag:'FLOW', conf:0.57},
              ].map((row,i) => (
                <div key={i} style={{
                  padding:'10px 12px',
                  background:'var(--surface-elev)',
                  border:'1px solid ' + (row.danger?'rgba(255, 77, 77, 0.32)':'var(--border-soft)'),
                  borderRadius:4,
                  position:'relative',
                }}>
                  <div style={{position:'absolute', left:-1, top:8, bottom:8, width:2, background: row.danger ? PAL.danger : PAL.gold, borderRadius:1, opacity:0.7}}></div>
                  <div style={{display:'flex', justifyContent:'space-between', alignItems:'flex-start', gap:8}}>
                    <div className="serif" style={{fontSize:14, color:'var(--text)', lineHeight:1.2, flex:1, minWidth:0}}>{row.title}</div>
                    <span className="mono" style={{color:'var(--text-3)', fontSize:10, flexShrink:0, marginTop:3}}>{row.t}</span>
                  </div>
                  <div style={{fontSize:11.5, color:'var(--text-2)', marginTop:5, lineHeight:1.45}}>{row.body}</div>
                  <div style={{display:'flex', justifyContent:'space-between', marginTop:6, alignItems:'center'}}>
                    <span className={'pill ' + (row.danger?'danger':'gold')} style={{fontSize:9}}>{row.tag}</span>
                    <span className="mono" style={{color:'var(--text-3)', fontSize:10}}>conf {(row.conf*100).toFixed(0)}%</span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        ) : (
          <div style={{display:'flex', flexDirection:'column', background:'var(--surface-card)', alignItems:'center', padding:'10px 0'}}>
            <button onClick={() => setLogOpen(true)} title="Show insight log"
              style={{background:'transparent', border:'1px solid var(--border)', color:'var(--text-2)', padding:'2px 7px', borderRadius:3, cursor:'pointer', fontSize:13, lineHeight:1, marginBottom:14}}>‹</button>
            <div style={{
              writingMode:'vertical-rl', transform:'rotate(180deg)',
              color:'var(--text-3)', fontSize:10, letterSpacing:'0.15em', textTransform:'uppercase',
              fontFamily:'Inter, sans-serif',
            }}>
              Insight Log · 6 events
            </div>
            <div style={{flex:1}}></div>
            <div style={{display:'flex', flexDirection:'column', gap:6, marginBottom:10}}>
              {['#00E676','#FF4D4D','#00E676','#00E676','#00E676','#00E676'].map((c,i) =>
                <span key={i} style={{width:8, height:8, borderRadius:'50%', background:c, opacity:0.7}}/>
              )}
            </div>
          </div>
        )}
      </div>

      {/* footer status */}
      <div style={{
        display:'flex', justifyContent:'space-between',
        padding:'8px 18px', fontSize:10.5, color:'var(--text-3)',
        borderTop:'1px solid var(--border-soft)', background:'var(--surface-sidebar)',
        fontFamily:'JetBrains Mono, monospace', letterSpacing:'0.04em', flex:'0 0 auto',
      }}>
        <span>EMA(21) · candle_pane · drag to pan · callouts follow candles</span>
        <span>{ANNOTATIONS.length} annotations · 6 indicators streaming · 1.8ms tick</span>
        <span>xvn-annot-v3 · build a7c2f1</span>
      </div>
    </div>
  );
};
