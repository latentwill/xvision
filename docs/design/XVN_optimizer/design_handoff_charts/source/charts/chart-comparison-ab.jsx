/* Chart 2 — Scalable Strategy Comparison (N strategies)
   1440×900 full dashboard chrome. Hero overlay equity (all selected),
   then a CSS-grid of strategy "cards" that wraps from 2 → 4 → 6 → 8.
   Each card has: mini equity sparkline + key metrics + indicator chips.
*/

window.ChartComparisonAB = function ChartComparisonAB(){
  const eq = React.useMemo(() => window.makeEquitySeries(220), []);
  const PAL = window.XVN_PALETTE;
  // pick 8 strategies — comparison scales nicely from 2 → 12
  const ALL = window.XVN_STRATEGIES;
  const [selectedIds, setSelectedIds] = React.useState(ALL.slice(0,6).map(s=>s.id));
  const selected = ALL.filter(s => selectedIds.includes(s.id));

  // ── hero overlay (uPlot) ──
  const heroRef = window.useUplot(parent => {
    const data = [eq.time, ...selected.map(s => eq.series[s.id])];
    return {
      width: parent.clientWidth,
      height: 280,
      padding: [14, 16, 0, 8],
      cursor: { drag:{x:true,y:false}, sync:{key:'cmp'}, points:{size:6, fill:(u,i)=>u.series[i].stroke()} },
      legend: { show: false },
      scales: { x: { time: true }, y: { auto: true } },
      axes: window.xvnAxes(),
      series: [
        {},
        ...selected.map((s, i) => window.xvnLine(s.short, s.color, { width: i===0?1.6:1.1, dashed: s.dashed })),
      ],
      data,
    };
  }, [selectedIds.join('|')]);

  // mini sparkline per card
  function MiniSpark({ strategy, dd }){
    const ref = window.useUplot(parent => {
      const series = eq.series[strategy.id];
      const ddSeries = window.makeDrawdownSeries(series);
      const h = Math.max(40, parent.clientHeight);
      return {
        width: parent.clientWidth,
        height: h,
        padding: [4, 4, 2, 4],
        cursor: { show:false },
        legend: { show:false },
        scales: { x:{time:true}, y:{auto:true} },
        axes: [{show:false},{show:false}],
        series: dd ? [
          {},
          { stroke: PAL.danger, width: 1, points:{show:false} },
        ] : [
          {},
          { stroke: strategy.color, width: 1.4, points:{show:false} },
        ],
        plugins: dd
          ? [ window.xvnAreaFill(1, 'rgba(255, 77, 77, 0.22)', 0) ]
          : [ window.xvnAreaFill(1, strategy.color + '22', 0) ],
        data: [eq.time, dd ? ddSeries : series],
      };
    }, [strategy.id, dd]);
    return <div ref={ref} style={{width:'100%', height:'100%', minHeight:48}}/>;
  }

  function StrategyCard({ s, lead, onRemove }){
    return (
      <div className="card" style={{
        display:'flex', flexDirection:'column',
        position:'relative', overflow:'hidden',
        background: lead ? 'linear-gradient(180deg, rgba(0, 230, 118, 0.04), var(--surface-card) 38%)' : 'var(--surface-card)',
        borderColor: lead ? 'rgba(0, 230, 118, 0.28)' : 'var(--border)',
      }}>
        {lead && <div style={{position:'absolute', top:0, left:0, right:0, height:1, background:'linear-gradient(to right, transparent, rgba(0, 230, 118, 0.7), transparent)'}}/>}
        {/* head */}
        <div style={{padding:'10px 12px 6px', display:'flex', alignItems:'flex-start', justifyContent:'space-between', gap:8}}>
          <div style={{minWidth:0, flex:1}}>
            <div style={{display:'flex', alignItems:'center', gap:7, minWidth:0}}>
              <span style={{flex:'0 0 auto', display:'inline-block', width:7, height:7, borderRadius:'50%', background:s.color, boxShadow:`0 0 0 3px ${s.color}1a`}}></span>
              <div className="serif" style={{fontSize:15.5, lineHeight:1.1, color:'var(--text)', whiteSpace:'nowrap', overflow:'hidden', textOverflow:'ellipsis'}}>{s.name}</div>
            </div>
            <div className="caps" style={{marginTop:4}}>{s.kind} · {s.short.split(' · ')[1] || 'live'}</div>
          </div>
          {lead && <span className="pill gold" style={{textTransform:'uppercase', fontSize:9, flex:'0 0 auto', padding:'2px 6px'}}>lead</span>}
          {!lead && onRemove && <button onClick={onRemove} className="btn ghost tiny" style={{padding:'1px 5px', fontSize:11, color:'var(--text-3)', flex:'0 0 auto', lineHeight:1}}>×</button>}
        </div>

        {/* equity spark */}
        <div style={{padding:'0 8px', flex:1, minHeight:0}}>
          <MiniSpark strategy={s}/>
        </div>

        {/* metrics 4 across */}
        <div style={{padding:'8px 12px 6px', display:'grid', gridTemplateColumns:'repeat(4,1fr)', gap:6, borderTop:'1px solid var(--border-soft)'}}>
          {[
            {l:'Return', v: window.fmtPct(s.return,1), c: s.return>=0?'var(--gold)':'var(--danger)'},
            {l:'Sharpe', v: s.sharpe.toFixed(2)},
            {l:'MaxDD',  v: s.mdd.toFixed(1)+'%', c:'var(--danger)'},
            {l:'Win',    v: s.win.toFixed(0)+'%'},
          ].map((m,i) => (
            <div key={i}>
              <div className="caps" style={{marginBottom:2, fontSize:9}}>{m.l}</div>
              <div className="mono" style={{fontSize:12.5, color: m.c || 'var(--text)'}}>{m.v}</div>
            </div>
          ))}
        </div>

        {/* indicator chips */}
        <div style={{padding:'6px 10px 9px', display:'flex', flexWrap:'wrap', gap:4, borderTop:'1px solid var(--border-soft)'}}>
          <span className="pill" style={{fontSize:9.5, padding:'2px 6px'}}><span style={{color:'var(--gold)'}}>RSI</span> 56</span>
          <span className="pill" style={{fontSize:9.5, padding:'2px 6px'}}><span style={{color:'var(--gold)'}}>MACD</span> ↑</span>
          <span className="pill" style={{fontSize:9.5, padding:'2px 6px'}}>EMA</span>
          <span className="pill ghost" style={{fontSize:9.5, padding:'2px 6px'}}>Fib · 0.618</span>
        </div>
      </div>
    );
  }

  // determine column count from selection size for clean wrap
  const n = selected.length;
  const cols = n <= 2 ? 2 : n <= 4 ? 4 : n <= 6 ? 3 : 4;

  return (
    <div className="shell" data-screen-label="02 Comparison AB (Scalable)">
      <aside className="sidebar">
        <div className="brand">xvn</div>
        <nav className="nav">
          {[
            {label:'Dashboard', icon:'◆'},
            {label:'Strategies', icon:'⋄'},
            {label:'Agents', icon:'◇'},
            {label:'Compare', active:true, icon:'⇄'},
            {label:'Eval', icon:'◊'},
            {label:'Docs', icon:'¶'},
            {label:'Settings', icon:'⚙'},
          ].map((m,i) => (
            <div key={i} className={'nav-item' + (m.active?' active':'')}>
              <span className="ic">{m.icon}</span>{m.label}
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

      <main className="main">
        <div className="topbar">
          <div>
            <div className="caps" style={{marginBottom:6}}>Compare · Side-by-Side</div>
            <h1>{selected.length} strategies, one frame</h1>
            <div className="sub">Scales from two to twelve. <span className="serif-i" style={{color:'var(--text)'}}>Add, remove, reorder; deltas update live.</span></div>
          </div>
          <div style={{display:'flex', gap:10, alignItems:'center'}}>
            <span className="pill"><span className="dot" style={{background:'var(--gold)'}}></span>paper · localhost</span>
            <div className="toggle-row">
              {['1M','3M','YTD','1Y','ALL'].map(t => <button key={t} className={t==='ALL'?'active':''}>{t}</button>)}
            </div>
            <button className="btn primary tiny">+ Add Strategy</button>
          </div>
        </div>

        {/* hero overlay */}
        <div className="card" style={{marginBottom:14}}>
          <div className="section-h">
            <div style={{display:'flex', alignItems:'baseline', gap:14}}>
              <h2>Equity Overlay</h2>
              <span className="caps">all selected · normalized to 0</span>
            </div>
            <div style={{display:'flex', gap:14, alignItems:'center', flexWrap:'wrap'}}>
              {selected.map(s => (
                <div key={s.id} className={'strat-chip' + (s.dashed?' dashed':'')} style={{color:s.color}}>
                  <span className="swatch"></span>
                  <span style={{color:'var(--text-2)'}}>{s.short.split(' · ')[0]}</span>
                  <span className="mono" style={{color:'var(--text-3)', fontSize:10.5}}>{window.fmtPct(s.return,1)}</span>
                </div>
              ))}
            </div>
          </div>
          <div ref={heroRef} style={{width:'100%', height:280}}/>
        </div>

        {/* selector ribbon */}
        <div style={{display:'flex', alignItems:'center', gap:8, marginBottom:12}}>
          <span className="caps" style={{marginRight:6}}>Roster</span>
          {ALL.map(s => {
            const on = selectedIds.includes(s.id);
            return (
              <button key={s.id} onClick={() => setSelectedIds(on ? selectedIds.filter(x=>x!==s.id) : [...selectedIds, s.id])}
                className="pill"
                style={{
                  cursor:'pointer',
                  background: on ? `${s.color}14` : 'transparent',
                  borderColor: on ? `${s.color}55` : 'var(--border)',
                  color: on ? s.color : 'var(--text-3)',
                  fontFamily:'Inter, sans-serif',
                  fontSize: 11,
                }}>
                <span style={{display:'inline-block', width:6, height:6, borderRadius:'50%', background:s.color, marginRight:6, opacity: on?1:0.35}}></span>
                {s.short.split(' · ')[0]}
              </button>
            );
          })}
        </div>

        {/* scalable grid */}
        <div style={{display:'grid', gridTemplateColumns:`repeat(${cols}, 1fr)`, gap:12, flex:1, minHeight:0}}>
          {selected.map((s, i) => (
            <StrategyCard key={s.id} s={s} lead={i===0}
              onRemove={selected.length > 2 ? () => setSelectedIds(selectedIds.filter(x=>x!==s.id)) : null}/>
          ))}
        </div>
      </main>
    </div>
  );
};
