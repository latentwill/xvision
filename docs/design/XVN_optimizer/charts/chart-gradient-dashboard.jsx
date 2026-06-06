/* Chart 5 — Gradient Warm Dashboard (xvn re-imagined)
   1440×900 full chrome. Subtle radial gold/ember "aura" background behind glass cards.
   Hero equity with multi-stop gold→ember vertical gradient fill (uPlot draw plugin).
   Right rail: performance radar (SVG) + KPI cards.
   Bottom row: drawdown overlap + monthly returns ribbon.
*/

window.ChartGradientDashboard = function ChartGradientDashboard(){
  const eq = React.useMemo(() => window.makeEquitySeries(260), []);
  const PAL = window.XVN_PALETTE;
  const strategies = window.XVN_STRATEGIES.slice(0, 5);
  const lead = strategies[0];

  // ── hero equity with gold→ember area fill on the lead, thin lines for others ──
  const heroRef = window.useUplot(parent => {
    const data = [eq.time, ...strategies.map(s => eq.series[s.id])];
    return {
      width: parent.clientWidth,
      height: 360,
      padding: [22, 16, 0, 4],
      cursor: { drag:{x:true,y:false}, points:{size:7, fill:(u,i)=>u.series[i].stroke()} },
      legend: { show: false },
      scales: { x:{time:true}, y:{auto:true} },
      axes: window.xvnAxes(),
      series: [
        {},
        // lead is heavy + receives gradient
        { label: lead.short, stroke: lead.color, width: 2.2, points:{show:false} },
        ...strategies.slice(1).map(s => window.xvnLine(s.short, s.color, { width: 1.05, dashed: s.dashed })),
      ],
      plugins: [
        // multi-stop warm gradient under the lead
        {
          hooks: {
            draw: u => {
              const ctx = u.ctx;
              const s = u.series[1];
              if (!s.show) return;
              const xData = u.data[0];
              const yData = u.data[1];
              const top = u.bbox.top, height = u.bbox.height;
              ctx.save();
              ctx.beginPath();
              let started = false;
              for (let i=0;i<xData.length;i++){
                const x = u.valToPos(xData[i], 'x', true);
                const y = u.valToPos(yData[i], 'y', true);
                if (!started){ ctx.moveTo(x, y); started=true; } else ctx.lineTo(x, y);
              }
              const lastX = u.valToPos(xData[xData.length-1], 'x', true);
              const firstX = u.valToPos(xData[0], 'x', true);
              const baseY = u.valToPos(u.scales.y.min, 'y', true);
              ctx.lineTo(lastX, baseY);
              ctx.lineTo(firstX, baseY);
              ctx.closePath();
              // warm vertical gradient: gold @ top → amber → ember → fade
              const grad = ctx.createLinearGradient(0, top, 0, top + height);
              grad.addColorStop(0.00, 'rgba(0, 230, 118, 0.42)');
              grad.addColorStop(0.25, 'rgba(94, 234, 212, 0.30)');
              grad.addColorStop(0.55, 'rgba(34, 211, 238, 0.18)');
              grad.addColorStop(0.85, 'rgba(34, 211, 238, 0.06)');
              grad.addColorStop(1.00, 'rgba(0,0,0,0)');
              ctx.fillStyle = grad;
              ctx.fill();
              // subtle horizontal sheen highlight
              const sheen = ctx.createLinearGradient(0, top, 0, top + height*0.4);
              sheen.addColorStop(0, 'rgba(255, 255, 255, 0.06)');
              sheen.addColorStop(1, 'rgba(255, 255, 255, 0)');
              ctx.fillStyle = sheen;
              ctx.fill();
              ctx.restore();
            },
          },
        },
        window.xvnLastDot(1, lead.color),
      ],
      data,
    };
  }, [eq]);

  // ── drawdown overlap (small) ──
  const ddRef = window.useUplot(parent => {
    const series = strategies.slice(0,3).map(s => window.makeDrawdownSeries(eq.series[s.id]));
    return {
      width: parent.clientWidth,
      height: 140,
      padding: [10, 12, 0, 8],
      cursor: { drag:{x:true,y:false}, points:{size:5} },
      legend:{show:false},
      scales: { x:{time:true}, y:{auto:true} },
      axes: window.xvnAxes({ yValues:(u,vals)=>vals.map(v => v.toFixed(0)+'%') }),
      series: [
        {},
        ...strategies.slice(0,3).map(s => ({ label: s.short, stroke: s.color, width: 1.2, points:{show:false} })),
      ],
      plugins: [
        window.xvnAreaFill(1, 'rgba(0, 230, 118, 0.18)', 0),
      ],
      data: [eq.time, ...series],
    };
  }, [eq]);

  return (
    <div className="shell" style={{position:'relative'}} data-screen-label="05 Gradient Warm Dashboard">

      {/* aura background washes */}
      <div className="aura" style={{width:520, height:520, top:-180, left:240, opacity:0.55}}></div>
      <div className="aura" style={{width:680, height:680, bottom:-260, right:-100, opacity:0.35, background:'radial-gradient(closest-side, rgba(34, 211, 238, 0.25), rgba(192, 132, 252, 0.08) 45%, transparent 75%)'}}></div>
      <div className="aura" style={{width:380, height:380, top:80, right:280, opacity:0.30, background:'radial-gradient(closest-side, rgba(94, 234, 212, 0.20), transparent 70%)'}}></div>

      {/* faint noise / grain overlay via repeating gradient */}
      <div style={{position:'absolute', inset:0, pointerEvents:'none', opacity:0.5,
        background:'repeating-linear-gradient(0deg, rgba(255, 255, 255, 0.012) 0 1px, transparent 1px 3px)'}}></div>

      {/* sidebar — keep crisp, not glassy */}
      <aside className="sidebar" style={{position:'relative', zIndex:1}}>
        <div className="brand">xvn</div>
        <nav className="nav">
          {[
            {label:'Dashboard', active:true, icon:'◆'},
            {label:'Strategies', icon:'⋄'},
            {label:'Agents', icon:'◇'},
            {label:'Scenarios', icon:'◈'},
            {label:'Eval', icon:'◊'},
            {label:'Docs', icon:'¶'},
            {label:'Settings', icon:'⚙'},
          ].map((n,i) => (
            <div key={i} className={'nav-item' + (n.active?' active':'')}>
              <span className="ic">{n.icon}</span>{n.label}
            </div>
          ))}
        </nav>
        <div className="user-row">
          <div className="avatar">DS</div>
          <div style={{flex:1}}>
            <div style={{fontSize:12, color:'var(--text)'}}>doss</div>
            <div style={{fontSize:11, color:'var(--text-3)'}}>paper · localhost</div>
          </div>
        </div>
      </aside>

      {/* main */}
      <main className="main" style={{position:'relative', zIndex:1}}>
        {/* topbar */}
        <div className="topbar">
          <div style={{maxWidth: 720, minWidth: 0}}>
            <div className="caps" style={{marginBottom:6, color: PAL.gold, letterSpacing:'0.18em'}}>Crypto · Strategy Hub</div>
            <h1 style={{fontSize:30, lineHeight:1.1, marginBottom:6, whiteSpace:'nowrap'}}>
              The <span className="serif-i" style={{
                background:'linear-gradient(90deg, #38BDF8 0%, #00E676 35%, #FB923C 80%)',
                WebkitBackgroundClip:'text', backgroundClip:'text',
                WebkitTextFillColor:'transparent', color:'transparent',
              }}>Golden Cross</span> is up <span className="mono" style={{color: PAL.gold, fontSize:26}}>82.41%</span>
            </h1>
            <div className="sub">18 months · 1,184 trades · <span className="serif-i" style={{color:'var(--text)'}}>one strategy carrying the book</span></div>
          </div>
          <div style={{display:'flex', gap:10, alignItems:'center'}}>
            <span className="pill gold">
              <span className="dot" style={{background:PAL.gold, boxShadow:'0 0 0 3px rgba(0, 230, 118, 0.18)'}}></span>
              paper · localhost
            </span>
            <div className="toggle-row">
              {['1D','1W','1M','3M','YTD','1Y','ALL'].map(t => <button key={t} className={t==='1Y'?'active':''}>{t}</button>)}
            </div>
            <button className="btn primary tiny">+ New Run</button>
          </div>
        </div>

        {/* KPI row — glass cards */}
        <div className="grid" style={{gridTemplateColumns:'repeat(5, 1fr)', gap:12, marginBottom:14}}>
          {[
            {l:'Total Return', v:'+82.41%', s:'annualized', c: PAL.gold, glow:true},
            {l:'Sharpe Ratio', v:'1.84',   s:'18-mo avg'},
            {l:'Sortino',      v:'2.41',   s:'downside σ'},
            {l:'Max Drawdown', v:'-18.72%', s:'Apr ’24', c: PAL.copper},
            {l:'Calmar Ratio', v:'4.40',   s:'return/MDD'},
          ].map((k,i) => (
            <div className="glass" key={i} style={{padding:'14px 16px', position:'relative', overflow:'hidden'}}>
              {k.glow && (
                <div style={{position:'absolute', inset:'-30px -30px auto auto', width:120, height:120,
                  background:'radial-gradient(closest-side, rgba(0, 230, 118, 0.30), transparent 70%)',
                  pointerEvents:'none'}}/>
              )}
              <div className="kpi-label">{k.l}</div>
              <div className="kpi-value" style={{color: k.c || 'var(--text)', fontSize:30}}>{k.v}</div>
              <div className="kpi-foot">{k.s}</div>
            </div>
          ))}
        </div>

        {/* hero + radar */}
        <div style={{display:'grid', gridTemplateColumns:'1fr 280px', gap:14, marginBottom:14}}>
          <div className="glass" style={{padding:0}}>
            <div className="section-h" style={{padding:'14px 18px 8px', borderBottom:'none'}}>
              <div style={{display:'flex', alignItems:'baseline', gap:14}}>
                <h2>Equity Curve</h2>
                <span className="caps">lead · {lead.short} · others overlaid</span>
              </div>
              <div style={{display:'flex', gap:18, alignItems:'center'}}>
                {strategies.map(s => (
                  <div key={s.id} className={'strat-chip' + (s.dashed?' dashed':'')} style={{color:s.color}}>
                    <span className="swatch"></span>
                    <span style={{color:'var(--text-2)'}}>{s.short.split(' · ')[0]}</span>
                  </div>
                ))}
              </div>
            </div>
            <div ref={heroRef} style={{width:'100%', height:360}}/>
          </div>

          <div className="glass" style={{padding:'14px 16px', display:'flex', flexDirection:'column'}}>
            <div className="caps" style={{marginBottom:10}}>Performance Radar</div>
            <PerfRadar strategies={strategies.slice(0,3)}/>
            <div style={{borderTop:'1px solid var(--border-soft)', paddingTop:10, marginTop:10}}>
              {[
                {l:'Return',      v: PAL.gold},
                {l:'Sharpe',      v: PAL.amber},
                {l:'Stability',   v: PAL.plum},
                {l:'Win Rate',    v: PAL.gold},
                {l:'Consistency', v: PAL.amber},
              ].map((a,i) => null)}
              <div style={{display:'flex', flexDirection:'column', gap:6}}>
                {strategies.slice(0,3).map(s => (
                  <div key={s.id} style={{display:'flex', justifyContent:'space-between', alignItems:'center', fontSize:11.5, color:'var(--text-2)'}}>
                    <span style={{display:'flex', alignItems:'center', gap:8, color: s.color}}>
                      <span style={{width:8, height:8, borderRadius:'50%', background:s.color, boxShadow:`0 0 0 2px ${s.color}28`}}/>
                      <span>{s.name.split(' ').slice(0,2).join(' ')}</span>
                    </span>
                    <span className="mono" style={{color:'var(--text)'}}>{window.fmtPct(s.return, 1)}</span>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>

        {/* bottom row */}
        <div style={{display:'grid', gridTemplateColumns:'1.3fr 1fr', gap:14, flex:1, minHeight:0}}>
          <div className="glass" style={{display:'flex', flexDirection:'column'}}>
            <div className="section-h" style={{padding:'12px 18px 4px', borderBottom:'none'}}>
              <h2>Drawdown Comparison</h2>
              <span className="caps">underwater · top 3</span>
            </div>
            <div ref={ddRef} style={{width:'100%', height:140}}/>
            <div style={{padding:'10px 18px 14px', display:'grid', gridTemplateColumns:'repeat(4,1fr)', gap:12, borderTop:'1px solid var(--border-soft)', marginTop:6}}>
              {[
                {l:'Worst Day',    v:'-6.84%', c: PAL.danger},
                {l:'Underwater',   v:'5 epochs'},
                {l:'Avg Recovery', v:'31 days'},
                {l:'Vol (Ann.)',   v:'36.31%'},
              ].map((m,i) => (
                <div key={i}>
                  <div className="caps" style={{marginBottom:4}}>{m.l}</div>
                  <div className="mono" style={{fontSize:14, color: m.c || 'var(--text)'}}>{m.v}</div>
                </div>
              ))}
            </div>
          </div>

          <div className="glass" style={{display:'flex', flexDirection:'column'}}>
            <div className="section-h" style={{padding:'12px 16px 6px', borderBottom:'none'}}>
              <h2>Market Context</h2>
              <span className="caps">BTC · spot</span>
            </div>
            <div style={{padding:'4px 16px 12px', display:'grid', gridTemplateColumns:'1fr 1fr', gap:10}}>
              <div>
                <div className="caps">Price</div>
                <div className="serif" style={{fontSize:24, color:'var(--text)', letterSpacing:'-0.01em'}}>$65,128</div>
                <div className="mono up" style={{fontSize:11}}>+1.84% · 24h</div>
              </div>
              <div>
                <div className="caps">Funding</div>
                <div className="serif" style={{fontSize:24, color: PAL.amber}}>+0.012%</div>
                <div className="mono" style={{fontSize:11, color:'var(--text-3)'}}>8h · 5-exch avg</div>
              </div>
              <div>
                <div className="caps">Open Interest</div>
                <div className="serif" style={{fontSize:24, color:'var(--text)'}}>$24.8B</div>
                <div className="mono" style={{fontSize:11, color:'var(--text-3)'}}>+2.1% · 24h</div>
              </div>
              <div>
                <div className="caps">Liq · 24h</div>
                <div className="serif" style={{fontSize:24, color: PAL.copper}}>$326M</div>
                <div className="mono" style={{fontSize:11, color:'var(--text-3)'}}>longs · 62%</div>
              </div>
            </div>
            <div style={{padding:'10px 16px 12px', borderTop:'1px solid var(--border-soft)', marginTop:'auto'}}>
              <div className="caps" style={{marginBottom:6}}>Regime</div>
              <div style={{display:'flex', gap:6, alignItems:'center'}}>
                <span className="pill gold" style={{fontSize:10.5}}>BULL · 62%</span>
                <span className="pill" style={{fontSize:10.5}}>SIDEWAYS · 22%</span>
                <span className="pill" style={{fontSize:10.5}}>BEAR · 9%</span>
                <span className="pill" style={{fontSize:10.5}}>HIGH VOL · 7%</span>
              </div>
            </div>
          </div>
        </div>
      </main>
    </div>
  );
};

// ── Performance Radar (pure SVG) ──
function PerfRadar({ strategies }){
  const PAL = window.XVN_PALETTE;
  const cx = 130, cy = 110, R = 78;
  const labels = ['Return','Sharpe','Stability','Win Rate','Consistency','Drawdown'];
  // synthesize per-strategy normalized values 0..1
  const valuesByStrat = strategies.map(s => {
    const r = Math.max(0, s.return/100 + 0.1);
    return [
      Math.min(1, r),
      Math.min(1, s.sharpe / 2.4),
      Math.min(1, 0.5 + (s.win-50)/100),
      Math.min(1, s.win / 70),
      Math.min(1, (s.pf - 0.8) / 1.2),
      Math.min(1, 1 - Math.abs(s.mdd)/35),
    ];
  });
  function point(i, v){
    const ang = -Math.PI/2 + (i / labels.length) * 2 * Math.PI;
    return [cx + Math.cos(ang) * R * v, cy + Math.sin(ang) * R * v];
  }
  return (
    <svg width="260" height="220" viewBox="0 0 260 220" style={{margin:'0 auto'}}>
      {/* grid rings */}
      {[0.25, 0.5, 0.75, 1].map(r => (
        <polygon key={r} points={labels.map((_,i) => point(i, r).join(',')).join(' ')}
          fill="none" stroke="rgba(255, 255, 255, 0.06)" strokeWidth="1"/>
      ))}
      {/* spokes */}
      {labels.map((_, i) => {
        const [x, y] = point(i, 1);
        return <line key={i} x1={cx} y1={cy} x2={x} y2={y} stroke="rgba(255, 255, 255, 0.06)"/>;
      })}
      {/* polygons per strategy */}
      {strategies.map((s, si) => (
        <g key={s.id}>
          <polygon points={valuesByStrat[si].map((v,i) => point(i,v).join(',')).join(' ')}
            fill={s.color} fillOpacity={0.10} stroke={s.color} strokeWidth="1.4"/>
          {valuesByStrat[si].map((v, i) => {
            const [x,y] = point(i,v);
            return <circle key={i} cx={x} cy={y} r={2.2} fill={s.color}/>;
          })}
        </g>
      ))}
      {/* labels */}
      {labels.map((label, i) => {
        const [x,y] = point(i, 1.18);
        return (
          <text key={i} x={x} y={y+3} textAnchor="middle"
            style={{fontFamily:'JetBrains Mono, monospace', fontSize:9, fill: PAL.mute, letterSpacing:'0.04em'}}>
            {label.toUpperCase()}
          </text>
        );
      })}
    </svg>
  );
}
