/* Chart 4 — Liquidation Heatmap (chart-only, 1200×680)
   KlineCharts candle pane with horizontal heat bands rendered as a sibling
   absolute overlay (independent of KlineCharts overlay engine), positioned
   via convertToPixel of price levels. Right rail lists the top levels.
*/

window.ChartLiquidationHeatmap = function ChartLiquidationHeatmap(){
  const PAL = window.XVN_PALETTE;
  const chartApiRef = React.useRef(null);
  const candles = React.useMemo(() => window.makeCandles(160, 63800, 31), []);
  const levels = React.useMemo(() => window.makeLiquidationLevels(candles), [candles]);
  const [recompute, setRecompute] = React.useState(0);

  const onLayout = React.useCallback((api) => {
    chartApiRef.current = api;
    setRecompute(r => r + 1);
  }, []);

  // bands re-derived from candle api
  const bands = React.useMemo(() => {
    const api = chartApiRef.current;
    if (!api) return [];
    return levels.map(l => ({
      ...l,
      y: api.yForPrice(l.price),
    })).filter(b => b.y >= api.layout.priceArea.y && b.y <= api.layout.priceArea.y + api.layout.priceArea.h);
  }, [recompute, levels]);

  // ── color ramp: red/orange (Bookmap-style), no gold ──
  function heatColor(heat){
    if (heat > 0.75) return { fill: '#FF4D4D', alpha: 0.48 };  // hot — coral-red
    if (heat > 0.55) return { fill: '#E04A3A', alpha: 0.42 };  // warm — red
    if (heat > 0.35) return { fill: '#FF4D4D', alpha: 0.36 };  // mid  — deep red
    if (heat > 0.15) return { fill: '#6A2A22', alpha: 0.30 };  // cool — dark red
    return                  { fill: '#3A1E1A', alpha: 0.22 };
  }

  return (
    <div data-screen-label="04 Liquidation Heatmap" style={{
      width:1200, height:680,
      background:'var(--bg)', color:'var(--text)',
      border:'1px solid var(--border)', borderRadius:8,
      display:'flex', flexDirection:'column', overflow:'hidden', position:'relative',
    }}>
      {/* header */}
      <div style={{
        display:'flex', alignItems:'center', justifyContent:'space-between',
        padding:'14px 18px', borderBottom:'1px solid var(--border-soft)',
        background:'var(--surface-card)',
      }}>
        <div style={{display:'flex', alignItems:'center', gap:14}}>
          <div className="serif-i" style={{fontSize:22, letterSpacing:'-0.02em'}}>xvn</div>
          <div className="divider-v"></div>
          <div>
            <div className="caps" style={{marginBottom:2}}>BTCUSDT · 1h · Liquidation Heatmap</div>
            <div style={{display:'flex', alignItems:'center', gap:8}}>
              <span className="serif" style={{fontSize:20, color:'var(--text)'}}>65,128.40</span>
              <span className="mono up" style={{fontSize:12}}>+1.84%</span>
              <span className="caps">notional · USD</span>
            </div>
          </div>
        </div>
        <div style={{display:'flex', gap:10, alignItems:'center'}}>
          <div className="toggle-row">
            {['1h','4h','1d','1w'].map(t => <button key={t} className={t==='1h'?'active':''}>{t}</button>)}
          </div>
          <span className="pill" style={{borderColor:'rgba(255, 77, 77, 0.4)', color:'#FF4D4D'}}><span className="dot" style={{background:'#FF4D4D'}}></span>longs at risk</span>
          <span className="pill ghost"><span className="dot" style={{background:'#FF4D4D'}}></span>shorts at risk</span>
        </div>
      </div>

      {/* body: chart + right rail */}
      <div style={{display:'grid', gridTemplateColumns:'1fr 280px', flex:1, minHeight:0}}>

        {/* chart + heat bands overlay */}
        <div style={{position:'relative', borderRight:'1px solid var(--border-soft)', overflow:'hidden'}}>
          {/* candle chart */}
          <XvnCandleChart candles={candles} ema={21} onLayout={onLayout}/>

          {/* heat band layer — overlays the candles */}
          <div style={{position:'absolute', inset:0, pointerEvents:'none', zIndex:2}}>
            {bands.map((b, i) => {
              const c = heatColor(b.heat);
              const h = 6 + b.heat * 18;
              return (
                <div key={i} className="heat-band" style={{
                  top: b.y - h/2,
                  height: h,
                  background: `linear-gradient(to right, transparent 0%, ${c.fill}${alpha(c.alpha)} 18%, ${c.fill}${alpha(c.alpha*1.4)} 60%, ${c.fill}${alpha(c.alpha*0.6)} 92%, transparent 100%)`,
                }}/>
              );
            })}
            {/* price tag overlays for the top 3 levels */}
            {bands.slice(0,3).map((b,i) => (
              <div key={'tag'+i} style={{
                position:'absolute', right:6, top:b.y-9,
                background:'rgba(10, 10, 10, 0.92)',
                border:'1px solid rgba(255, 77, 77, 0.4)',
                padding:'2px 6px', fontSize:10, color: '#FF4D4D',
                fontFamily:'JetBrains Mono, monospace',
                borderRadius:2,
              }}>
                ${b.notional}M · ${b.price.toFixed(0)}
              </div>
            ))}
          </div>
        </div>

        {/* right rail */}
        <div style={{display:'flex', flexDirection:'column', background:'var(--surface-card)'}}>
          <div className="section-h" style={{padding:'12px 14px 10px'}}>
            <h2 style={{fontSize:17}}>Top Liquidation Levels</h2>
            <span className="caps">24h · estimated</span>
          </div>
          <div style={{padding:'4px 14px 8px'}}>
            <div style={{display:'grid', gridTemplateColumns:'52px 1fr 60px', columnGap:8, color:'var(--text-3)', fontSize:9.5, letterSpacing:'0.1em', textTransform:'uppercase', paddingBottom:6, borderBottom:'1px solid var(--border-soft)'}}>
              <span>Price</span><span>Heat</span><span style={{textAlign:'right'}}>USD</span>
            </div>
            <div style={{display:'flex', flexDirection:'column', gap:0}}>
              {levels.slice(0,10).map((l,i) => {
                const c = heatColor(l.heat);
                return (
                  <div key={i} className="heat-rail-row">
                    <span style={{color:'var(--text-2)'}}>${l.price.toFixed(0)}</span>
                    <span className="heat-rail-bar" style={{color: c.fill, opacity: 0.4 + l.heat*0.7}}></span>
                    <span style={{textAlign:'right', color: l.side==='long' ? '#FF4D4D' : '#FF4D4D'}}>${l.notional}M</span>
                  </div>
                );
              })}
            </div>
          </div>

          <div style={{padding:'12px 14px', borderTop:'1px solid var(--border-soft)'}}>
            <div className="caps" style={{marginBottom:8}}>Cascade Analytics</div>
            <div style={{display:'grid', gridTemplateColumns:'1fr 1fr', gap:10}}>
              <div>
                <div className="caps" style={{fontSize:9.5}}>Long Exposure</div>
                <div className="mono" style={{fontSize:15, color: '#FF4D4D'}}>$182.4M</div>
              </div>
              <div>
                <div className="caps" style={{fontSize:9.5}}>Short Exposure</div>
                <div className="mono" style={{fontSize:15, color: '#FF4D4D'}}>$143.7M</div>
              </div>
              <div>
                <div className="caps" style={{fontSize:9.5}}>Nearest Wall</div>
                <div className="mono" style={{fontSize:13, color:'var(--text)'}}>65,820 · +1.1%</div>
              </div>
              <div>
                <div className="caps" style={{fontSize:9.5}}>Cascade Risk</div>
                <div className="mono" style={{fontSize:13, color: '#E04A3A'}}>Elevated · 0.62</div>
              </div>
            </div>
          </div>

          {/* heat legend */}
          <div style={{padding:'12px 14px', borderTop:'1px solid var(--border-soft)', marginTop:'auto'}}>
            <div className="caps" style={{marginBottom:8}}>Heat Scale</div>
            <div style={{height:8, borderRadius:1, background:'linear-gradient(to right, rgba(40, 14, 14, 0.3), rgba(120, 30, 30, 0.5), rgba(200, 50, 50, 0.7), rgba(255, 77, 77, 0.9), rgba(255,107,92,1))'}}></div>
            <div style={{display:'flex', justifyContent:'space-between', marginTop:4, fontSize:9.5, color:'var(--text-3)', fontFamily:'JetBrains Mono, monospace'}}>
              <span>cold</span><span>warm</span><span>hot</span><span>scorching</span>
            </div>
          </div>
        </div>
      </div>

      {/* footer */}
      <div style={{
        display:'flex', justifyContent:'space-between',
        padding:'8px 18px', fontSize:10.5, color:'var(--text-3)',
        borderTop:'1px solid var(--border-soft)', background:'var(--surface-sidebar)',
        fontFamily:'JetBrains Mono, monospace', letterSpacing:'0.04em',
      }}>
        <span>levels sourced · coinglass + xvn risk engine</span>
        <span>{bands.length} bands rendered · screen blend</span>
        <span>updated · 1.8s ago</span>
      </div>
    </div>
  );
};

function alpha(a){
  // convert alpha (0..1) → 2-digit hex
  const n = Math.max(0, Math.min(255, Math.round(a * 255)));
  return n.toString(16).padStart(2,'0');
}
