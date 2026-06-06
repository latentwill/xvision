/* Chart 1 — Dark Minimal Strategy Dashboard (xvn-themed)
   1440×900 full dashboard chrome. Single hero equity curve (uPlot multi-line),
   under it: drawdown sparkline + monthly returns mini-grid + strategy table.
*/

window.ChartDarkMinimal = function ChartDarkMinimal(){
  const eq = React.useMemo(() => window.makeEquitySeries(260), []);
  const PAL = window.XVN_PALETTE;
  const strategies = window.XVN_STRATEGIES.slice(0, 5); // fib, ema, brk, msw, btc(hold)
  const tableStrats = [strategies[0], strategies[1], strategies[2], strategies[3], window.XVN_STRATEGIES[7]];

  // ── hero equity uPlot ──
  const heroRef = window.useUplot(parent => {
    const data = [eq.time, ...tableStrats.map(s => eq.series[s.id])];
    const lead = tableStrats[0];
    return {
      width: parent.clientWidth,
      height: 320,
      padding: [16, 18, 0, 8],
      cursor: {
        drag: { x: true, y: false },
        sync: { key: 'hero' },
        points: { size: 7, fill: (u, sIdx) => u.series[sIdx].stroke() },
      },
      legend: { show: false },
      scales: { x: { time: true }, y: { auto: true } },
      axes: window.xvnAxes(),
      series: [
        {},
        ...tableStrats.map((s, i) => window.xvnLine(s.short, s.color, { width: i===0 ? 1.7 : 1.15, dashed: s.dashed })),
      ],
      plugins: [
        window.xvnLastDot(1, lead.color),
      ],
      data,
    };
  }, [eq]);

  // ── drawdown sparkline ──
  const ddRef = window.useUplot(parent => {
    const lead = tableStrats[0];
    const dd = window.makeDrawdownSeries(eq.series[lead.id]);
    return {
      width: parent.clientWidth,
      height: 120,
      padding: [6, 12, 0, 8],
      cursor: { drag:{x:true,y:false}, points:{size:5,fill:lead.color} },
      legend: { show: false },
      scales: { x: { time: true }, y: { auto: true } },
      axes: window.xvnAxes({ yValues: (u,vals)=>vals.map(v => v.toFixed(0)+'%') }),
      series: [
        {},
        { label: 'DD', stroke: PAL.danger, width: 1.2, points:{show:false} },
      ],
      plugins: [
        window.xvnAreaFill(1, 'rgba(255, 77, 77, 0.22)', 0),
      ],
      data: [eq.time, dd],
    };
  }, [eq]);

  // ── monthly returns ──
  const monthly = React.useMemo(() => window.makeMonthlyMatrix(17), []);
  const months = ['Jan','Feb','Mar','Apr','May','Jun','Jul','Aug','Sep','Oct','Nov','Dec','Jan','Feb','Mar','Apr','May'];

  return (
    <div className="shell" data-screen-label="01 Dark Minimal Strategy Dashboard">
      {/* sidebar */}
      <aside className="sidebar">
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
            <div key={i} className={'nav-item' + (n.active ? ' active':'')}>
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
      <main className="main">
        {/* topbar */}
        <div className="topbar">
          <div>
            <div className="caps" style={{marginBottom:6}}>Dashboard · Strategy Comparison</div>
            <h1>Strategy Comparison</h1>
            <div className="sub">Five strategies. Eighteen months. <span className="serif-i" style={{color:'var(--text)'}}>Compounding compounded.</span></div>
          </div>
          <div style={{display:'flex', gap:10, alignItems:'center'}}>
            <span className="pill"><span className="dot" style={{background:'var(--gold)'}}></span>paper · localhost</span>
            <div className="toggle-row">
              {['1D','1W','1M','3M','YTD','1Y','ALL'].map(t => (
                <button key={t} className={t==='ALL'?'active':''}>{t}</button>
              ))}
            </div>
            <button className="btn ghost tiny">⤓ Export</button>
          </div>
        </div>

        {/* KPI row */}
        <div className="grid" style={{gridTemplateColumns:'repeat(5, 1fr)', gap:12, marginBottom:16}}>
          {[
            {l:'Total Return', v:'+82.41%', s:'annualized', up:true},
            {l:'Sharpe Ratio', v:'1.84',   s:'18-mo avg'},
            {l:'Max Drawdown', v:'-18.72%', s:'Apr — Jun ’24', down:true},
            {l:'Win Rate', v:'56.3%',     s:'1,184 trades'},
            {l:'Profit Factor', v:'1.67',  s:'gross/gross'},
          ].map((k,i) => (
            <div className="kpi" key={i}>
              <div className="kpi-label">{k.l}</div>
              <div className="kpi-value" style={{color: k.up ? 'var(--gold)' : k.down ? 'var(--danger)' : 'var(--text)'}}>{k.v}</div>
              <div className="kpi-foot">{k.s}</div>
            </div>
          ))}
        </div>

        {/* hero equity */}
        <div className="card" style={{marginBottom:14}}>
          <div className="section-h">
            <div style={{display:'flex', alignItems:'baseline', gap:14}}>
              <h2>Equity Curve</h2>
              <span className="caps">multi-strategy overlay · synced cursor</span>
            </div>
            <div style={{display:'flex', gap:18, alignItems:'center'}}>
              {tableStrats.map(s => (
                <div key={s.id} className={'strat-chip' + (s.dashed ? ' dashed':'')} style={{color: s.color}}>
                  <span className="swatch"></span>
                  <span style={{color:'var(--text-2)'}}>{s.short}</span>
                </div>
              ))}
            </div>
          </div>
          <div ref={heroRef} style={{width:'100%', height:320}}/>
        </div>

        {/* lower row: drawdown + monthly + table-ish */}
        <div className="grid" style={{gridTemplateColumns:'1fr 1.2fr', gap:14, flex:1, minHeight:0}}>

          <div className="card" style={{display:'flex', flexDirection:'column'}}>
            <div className="section-h">
              <h2>Drawdown · Fibonacci GC</h2>
              <span className="caps">peak-to-trough · underwater</span>
            </div>
            <div ref={ddRef} style={{width:'100%', height:120}}/>
            <div style={{padding:'12px 18px 14px', display:'grid', gridTemplateColumns:'repeat(4,1fr)', gap:12, borderTop:'1px solid var(--border-soft)'}}>
              {[
                {l:'Max DD', v:'-18.72%', c:'var(--danger)'},
                {l:'Avg DD', v:'-6.42%'},
                {l:'Duration', v:'48 days'},
                {l:'Recovery', v:'36 days'},
              ].map((m,i) => (
                <div key={i}>
                  <div className="caps" style={{marginBottom:4}}>{m.l}</div>
                  <div className="mono" style={{fontSize:14, color: m.c || 'var(--text)'}}>{m.v}</div>
                </div>
              ))}
            </div>
          </div>

          <div className="card" style={{display:'flex', flexDirection:'column'}}>
            <div className="section-h">
              <h2>Monthly Returns</h2>
              <span className="caps">strategy × month · % return</span>
            </div>
            <div style={{padding:'12px 18px 14px', flex:1, overflow:'hidden'}}>
              <div style={{display:'grid', gridTemplateColumns:'88px repeat(17, 1fr)', gap:2, fontFamily:'JetBrains Mono, monospace', fontSize:10}}>
                <div></div>
                {months.map((m,i) => (
                  <div key={i} style={{color:'var(--text-3)', textAlign:'center', paddingBottom:4}}>{m}</div>
                ))}
                {monthly.map((row, ri) => (
                  <React.Fragment key={ri}>
                    <div style={{color:'var(--text-2)', fontSize:11, paddingRight:8, display:'flex', alignItems:'center'}}>
                      {row.strategy.short.split(' · ')[0]}
                    </div>
                    {row.values.map((v, ci) => {
                      const abs = Math.min(Math.abs(v) / 0.18, 1);
                      const color = v >= 0
                        ? `rgba(0, 230, 118, ${0.10 + abs*0.55})`
                        : `rgba(255, 77, 77, ${0.10 + abs*0.55})`;
                      return (
                        <div key={ci} title={(v*100).toFixed(2)+'%'} style={{
                          background: color,
                          height: 22,
                          display:'flex', alignItems:'center', justifyContent:'center',
                          color: 'var(--text)',
                          fontSize: 9.5,
                          border:'1px solid rgba(0,0,0,0.25)',
                        }}>
                          {(v*100).toFixed(1)}
                        </div>
                      );
                    })}
                  </React.Fragment>
                ))}
              </div>
              {/* legend */}
              <div style={{display:'flex', justifyContent:'flex-end', alignItems:'center', gap:8, marginTop:14, color:'var(--text-3)', fontSize:10.5}}>
                <span>-15%</span>
                <div style={{width:140, height:6, background:'linear-gradient(to right, rgba(255, 77, 77, 0.65), rgba(255, 255, 255, 0.05) 50%, rgba(0, 230, 118, 0.65))', borderRadius:1}}></div>
                <span>+15%</span>
              </div>
            </div>
          </div>
        </div>
      </main>
    </div>
  );
};
