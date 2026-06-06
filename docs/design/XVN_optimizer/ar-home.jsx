// Autoresearch · home (/research)
// Live dashboard: tonight's running cycle, population in mutation, recent cycles.

const ARHome = () => {
  const cyc = AR_CURRENT_CYCLE;

  return (
    <Frame>
      <SideNav active="optimizer" marketplaceVisible={true} optimizerVisible={true}/>
      <main style={{display:"flex", flexDirection:"column", minWidth:0, overflow:"hidden"}}>
        <TopStatus breadcrumb={[{ text:"OPTIMIZER" }]}/>

        {/* ── Page header ── */}
        <div style={{
          padding:"20px 28px 16px", borderBottom:"1px solid var(--border)",
          display:"flex", justifyContent:"space-between", alignItems:"flex-end", gap:24,
        }}>
          <div style={{minWidth:0, maxWidth:780}}>
            <div style={{display:"flex", alignItems:"center", gap:8, marginBottom:8}}>
              <span className="ulabel" style={{fontSize:9.5, letterSpacing:"0.22em"}}>OPTIMIZER</span>
              <span style={{color:"var(--text-4)"}}>·</span>
              <ARStatusPill status="breeding"/>
            </div>
            <h1 style={{
              margin:0, fontSize:24, fontWeight:600, letterSpacing:"-0.025em", lineHeight:1.15,
            }}>Tonight's evening run is in progress. <span style={{color:"var(--text-3)"}}>1 cycle running · 5 active lineages.</span></h1>
            <div className="mono" style={{
              marginTop:10, fontSize:11.5, color:"var(--text-3)", letterSpacing:"0.01em",
            }}>
              <span><span style={{color:"var(--text-2)"}}>54</span> experiments tonight</span>
              <span style={{margin:"0 10px", color:"var(--text-4)"}}>·</span>
              <span><span style={{color:"var(--gold)"}}>7</span> kept this week</span>
              <span style={{margin:"0 10px", color:"var(--text-4)"}}>·</span>
              <span><span style={{color:"var(--text-2)"}}>31.8M</span> tokens this week</span>
              <span style={{margin:"0 10px", color:"var(--text-4)"}}>·</span>
              <span><span style={{color:"var(--gold)"}}>$15.57</span> LLM spend this week</span>
            </div>
          </div>
          <div style={{display:"flex", gap:8, alignItems:"center"}}>
            <Btn variant="ghost" icon="cog">Configure loop</Btn>
            <Btn variant="ghost" icon="info">What is this?</Btn>
            <Btn variant="primary" icon="plus">Trigger off-cycle run</Btn>
          </div>
        </div>

        {/* ── Body ── */}
        <div style={{flex:1, minHeight:0, overflow:"auto"}}>
          {/* In-flight cycle hero */}
          <CycleInFlight cyc={cyc}/>

          {/* Population */}
          <div style={{padding:"18px 28px 8px"}}>
            <SectionHeader title="Active lineages"
              sub="5 lineages running · 1 cooled · 1 paused · click a card to drill into tonight's experiments"
              right={<>
                <Btn variant="ghost" dense icon="plus">Add lineage</Btn>
                <Btn variant="ghost" dense icon="ext">View all</Btn>
              </>}/>
            <div style={{
              display:"grid", gridTemplateColumns:"repeat(3, 1fr)", gap:12, marginTop:14,
            }}>
              {AR_POPULATION.map((p) => <PopulationCard key={p.lineage} p={p}/>)}
            </div>
          </div>

          {/* Recent cycles */}
          <div style={{padding:"22px 28px 28px"}}>
            <SectionHeader title="Recent cycles"
              sub="last 5 nights · kept experiments landed in Marketplace"
              right={<Btn variant="ghost" dense icon="ext">Open ledger</Btn>}/>
            <div style={{
              marginTop:12, border:"1px solid var(--border)", borderRadius:6,
            }}>
              <RecentCyclesTable/>
            </div>
          </div>
        </div>
      </main>
    </Frame>
  );
};

// ── Hero: in-flight cycle ──
const CycleInFlight = ({ cyc }) => {
  // 14 variants × 5 regimes = 70 cells. Build the grid.
  const cells = [];
  for (let v = 0; v < cyc.variants.length; v++) {
    for (let r = 0; r < 5; r++) {
      const variant = cyc.variants[v];
      let state;
      if (variant.status === "done")     state = "done";
      else if (variant.status === "failed") state = r === 3 ? "failed" : "done"; // failed on flash-crash
      else if (variant.status === "running") {
        // first N regimes done, current = running, rest queued
        const idx = parseInt(variant.summary.match(/(\d) of 5/)?.[1] || "0", 10);
        if (r < idx) state = "done";
        else if (r === idx) state = "running";
        else state = "queued";
      }
      else state = "queued";
      cells.push({ v, r, state, experiment:variant.experiment, deltaSharpe:variant.deltaSharpe });
    }
  }

  return (
    <div style={{
      padding:"20px 28px", borderBottom:"1px solid var(--border)",
      display:"grid", gridTemplateColumns:"316px 1fr 280px", gap:24, alignItems:"stretch",
    }}>
      {/* Left: dial + parent + counters */}
      <div style={{
        border:"1px solid var(--gold-soft)",
        background:"linear-gradient(180deg, rgba(0,230,118,0.05), rgba(0,230,118,0.01))",
        borderRadius:6, padding:"16px 16px",
        display:"flex", flexDirection:"column", gap:14,
      }}>
        <div style={{display:"flex", alignItems:"center", gap:14}}>
          <CycleDial progress={cyc.progress} size={68} stroke={6} label="CYCLE"/>
          <div style={{minWidth:0}}>
            <div className="ulabel" style={{fontSize:9, letterSpacing:"0.2em"}}>EVENING RUN · IN PROGRESS</div>
            <div className="mono" style={{fontSize:13, color:"var(--text)", marginTop:3, fontWeight:600}}>
              {cyc.id}
            </div>
            <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginTop:3}}>
              elapsed <span style={{color:"var(--text-2)"}}>{cyc.elapsed}</span>
              <span style={{margin:"0 4px", color:"var(--text-4)"}}>·</span>
              eta <span style={{color:"var(--gold)"}}>{cyc.remaining}</span>
            </div>
          </div>
        </div>

        <div style={{borderTop:"1px solid var(--border)", paddingTop:12, display:"flex", alignItems:"center", gap:12}}>
          <GenArt seed={cyc.parentSeed} size={44} style={{borderRadius:5, border:"1px solid var(--border)"}}/>
          <div style={{minWidth:0}}>
            <div className="ulabel" style={{fontSize:9, letterSpacing:"0.18em"}}>PARENT</div>
            <div className="mono" style={{fontSize:12, color:"var(--text)", marginTop:3, fontWeight:600}}>
              {cyc.parent}
            </div>
            <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginTop:2}}>
              parent sharpe <span style={{color:"var(--text)"}}>{cyc.parentSharpe.toFixed(2)}</span>
            </div>
          </div>
        </div>

        <div style={{display:"grid", gridTemplateColumns:"1fr 1fr", gap:10}}>
          <SmallStat label="EXPERIMENTS" value="14" sub="7 done · 3 testing · 3 queued · 1 failed"/>
          <SmallStat label="EVALS"    value={`${cyc.evalsDone}/${cyc.evalsTotal}`} sub={`${cyc.evalsFailed} retried`}/>
          <SmallStat label="LLM CALLS" value={cyc.llmCalls} sub={`${cyc.tokensSpent} tokens`}/>
          <SmallStat label="$ SPEND" value={cyc.costUSD} sub="local · backtest only"/>
        </div>

        <button style={{
          marginTop:"auto", padding:"7px 12px", borderRadius:4,
          border:"1px solid var(--border-strong)", background:"transparent",
          color:"var(--text-2)", fontSize:12, fontFamily:"'Geist', sans-serif",
          cursor:"pointer", display:"flex", alignItems:"center", justifyContent:"center", gap:6,
        }}>
          <span style={{
            width:8, height:8, borderRadius:1, border:"2px solid currentColor",
          }}/>
          Pause cycle
        </button>
      </div>

      {/* Middle: progress matrix — variants × regimes heatmap */}
      <div style={{
        border:"1px solid var(--border)", borderRadius:6,
        background:"var(--surface-card)", padding:"14px 16px 12px",
        display:"flex", flexDirection:"column", minWidth:0,
      }}>
        <div style={{display:"flex", justifyContent:"space-between", alignItems:"baseline", marginBottom:10}}>
          <div>
            <div style={{fontSize:13.5, fontWeight:600, color:"var(--text)"}}>
              Live progress · experiments × regimes
            </div>
            <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginTop:3}}>
              each cell is one backtest · testing cells animate · click to inspect
            </div>
          </div>
          <div style={{display:"flex", gap:10}}>
            <LegendDot color="var(--gold)"   label="done"/>
            <LegendDot color="var(--info)"   label="testing"/>
            <LegendDot color="var(--text-4)" label="queued"/>
            <LegendDot color="var(--danger)" label="failed"/>
          </div>
        </div>

        {/* Regime column headers */}
        <div style={{
          display:"grid", gridTemplateColumns:`80px repeat(5, 1fr)`, gap:6, marginBottom:5,
        }}>
          <div/>
          {AR_REGIMES.map((r) => (
            <div key={r.id} style={{
              display:"flex", alignItems:"center", gap:5, justifyContent:"center",
            }}>
              <RegimeIcon kind={r.kind} size={10} color={REGIME_KIND_COLOR[r.kind]}/>
              <span className="mono" style={{
                fontSize:9.5, color:"var(--text-3)", letterSpacing:"0.04em",
              }}>{r.label}</span>
            </div>
          ))}
        </div>

        {/* Rows */}
        <div style={{display:"flex", flexDirection:"column", gap:3}}>
          {cyc.variants.map((v, vi) => (
            <div key={v.id} style={{
              display:"grid", gridTemplateColumns:`80px repeat(5, 1fr)`, gap:6, alignItems:"center",
            }}>
              <div style={{display:"flex", alignItems:"center", gap:5, minWidth:0}}>
                <span className="mono" style={{
                  fontSize:10.5, color: v.status === "queued" ? "var(--text-4)" : "var(--text-2)",
                  fontWeight:600,
                }}>{v.id}</span>
              </div>
              {[0,1,2,3,4].map((ri) => {
                const cell = cells.find((c) => c.v === vi && c.r === ri);
                return <HeatCell key={ri} state={cell.state} sharpe={cell.deltaSharpe}/>;
              })}
            </div>
          ))}
        </div>

        <div style={{
          marginTop:10, paddingTop:8, borderTop:"1px solid var(--border-soft)",
          display:"flex", alignItems:"center", gap:12,
        }}>
          <ARProgressBar value={cyc.evalsDone} total={cyc.evalsTotal}/>
          <span className="mono" style={{fontSize:10.5, color:"var(--text-3)", whiteSpace:"nowrap"}}>
            <span style={{color:"var(--text-2)"}}>{cyc.evalsDone}</span>
            <span style={{color:"var(--text-4)"}}>/</span>
            <span style={{color:"var(--text-3)"}}>{cyc.evalsTotal}</span> evals
          </span>
        </div>
      </div>

      {/* Right: kept experiments today */}
      <div style={{
        border:"1px solid var(--border)", borderRadius:6,
        padding:"14px 14px 12px",
        display:"flex", flexDirection:"column", gap:10,
      }}>
        <div style={{display:"flex", alignItems:"center", justifyContent:"space-between"}}>
          <div>
            <div style={{fontSize:13.5, fontWeight:600}}>Kept</div>
            <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginTop:3}}>
              passed the gate · so far this run
            </div>
          </div>
          <div className="mono" style={{fontSize:22, color:"var(--gold)", fontWeight:600, letterSpacing:"-0.02em"}}>
            3
          </div>
        </div>

        <div style={{display:"flex", flexDirection:"column", gap:8}}>
          {AR_CURRENT_CYCLE.variants.filter((v) => v.kept).map((v) => (
            <div key={v.id} style={{
              padding:"9px 10px", borderRadius:5,
              border:"1px solid var(--gold-soft)", background:"var(--gold-bg)",
              display:"flex", alignItems:"center", gap:10,
            }}>
              <GenArt seed={v.seed} size={36} style={{borderRadius:4, border:"1px solid rgba(0,0,0,0.4)"}}/>
              <div style={{flex:1, minWidth:0}}>
                <div style={{display:"flex", alignItems:"center", gap:6}}>
                  <span className="mono" style={{fontSize:11.5, fontWeight:600, color:"var(--text)"}}>{v.id}</span>
                  <ExperimentPill kind={v.experiment} withLabel={false}/>
                </div>
                <DeltaSharpeCell value={v.deltaSharpe}/>
              </div>
              <Icon name="check" size={14} color="var(--gold)" sw={2}/>
            </div>
          ))}
        </div>

        <div style={{
          marginTop:"auto", padding:"10px 11px",
          border:"1px dashed var(--border-strong)", borderRadius:5,
        }}>
          <div className="ulabel" style={{fontSize:9, letterSpacing:"0.2em", marginBottom:5}}>NEXT</div>
          <div className="mono" style={{fontSize:11, color:"var(--text-2)", lineHeight:1.5}}>
            evening summary signed at <span style={{color:"var(--gold)"}}>06:14</span> · bundle committed locally
            <br/>
            <span style={{color:"var(--text-3)"}}>publish to chain → optional · runs from Marketplace</span>
          </div>
        </div>
      </div>
    </div>
  );
};

// ── Reusable sub-components ──

const SectionHeader = ({ title, sub, right }) => (
  <div style={{display:"flex", alignItems:"flex-end", justifyContent:"space-between"}}>
    <div>
      <h2 style={{margin:0, fontSize:16, fontWeight:600, letterSpacing:"-0.015em"}}>{title}</h2>
      {sub && <div className="mono" style={{fontSize:11, color:"var(--text-3)", marginTop:4}}>{sub}</div>}
    </div>
    {right && <div style={{display:"flex", gap:8}}>{right}</div>}
  </div>
);

const SmallStat = ({ label, value, sub }) => (
  <div style={{
    padding:"8px 10px", border:"1px solid var(--border-soft)", borderRadius:4,
    background:"rgba(0,0,0,0.3)",
  }}>
    <div className="ulabel" style={{fontSize:8.5, letterSpacing:"0.18em"}}>{label}</div>
    <div className="mono" style={{fontSize:14, fontWeight:600, color:"var(--text)", marginTop:3, lineHeight:1}}>
      {value}
    </div>
    {sub && <div className="mono" style={{fontSize:9.5, color:"var(--text-3)", marginTop:4}}>{sub}</div>}
  </div>
);

const LegendDot = ({ color, label }) => (
  <span style={{display:"inline-flex", alignItems:"center", gap:5}}>
    <span style={{width:7, height:7, borderRadius:1.5, background:color}}/>
    <span className="mono" style={{fontSize:10, color:"var(--text-3)", letterSpacing:"0.04em"}}>{label}</span>
  </span>
);

const HeatCell = ({ state, sharpe }) => {
  const styles = {
    done:    { bg:"var(--gold-bg-strong)", bd:"var(--gold-soft)",      label:null },
    running: { bg:"rgba(95,168,255,0.18)", bd:"rgba(95,168,255,0.50)", label:"…" },
    queued:  { bg:"var(--surface-elev)",   bd:"var(--border)",         label:null },
    failed:  { bg:"rgba(255,77,77,0.16)",  bd:"rgba(255,77,77,0.50)",  label:"x" },
  };
  const s = styles[state] || styles.queued;
  return (
    <div style={{
      height:18, borderRadius:2,
      background:s.bg, border:`1px solid ${s.bd}`, position:"relative",
      display:"flex", alignItems:"center", justifyContent:"center",
      overflow:"hidden",
    }}>
      {state === "running" && (
        <div className="bar-flow" style={{
          position:"absolute", inset:0,
          background:"linear-gradient(90deg, transparent, rgba(95,168,255,0.35), transparent)",
        }}/>
      )}
      {s.label && (
        <span className="mono" style={{
          fontSize:9, color: state === "running" ? "var(--info)" : "var(--danger)",
          letterSpacing:"0.1em", fontWeight:700, position:"relative",
        }}>{s.label}</span>
      )}
    </div>
  );
};

// ── Population card ──
const PopulationCard = ({ p }) => {
  const statusCol = p.status === "breeding" ? "var(--gold)" :
                    p.status === "cooled"   ? "var(--text-3)" :
                                              "var(--warn)";
  return (
    <div style={{
      padding:"14px 14px", border:"1px solid var(--border)", borderRadius:6,
      background:"var(--surface-card)", cursor:"pointer",
      display:"flex", flexDirection:"column", gap:10,
    }}>
      <div style={{display:"flex", alignItems:"center", gap:11}}>
        <GenArt seed={p.seed} size={48} style={{borderRadius:5, border:"1px solid var(--border)"}}/>
        <div style={{flex:1, minWidth:0}}>
          <div style={{display:"flex", alignItems:"center", gap:7}}>
            <span className="mono" style={{
              fontSize:13.5, color:"var(--text)", fontWeight:600,
            }}>{p.lineage}</span>
            <ARStatusPill status={p.status}/>
          </div>
          <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginTop:3}}>
            parent <span style={{color:"var(--text-2)"}}>{p.parent}</span>
            <span style={{margin:"0 5px", color:"var(--text-4)"}}>·</span>
            sharpe <span style={{color:"var(--text-2)"}}>{p.parentSharpe.toFixed(2)}</span>
          </div>
        </div>
      </div>

      <div style={{
        display:"grid", gridTemplateColumns:"1fr 1fr 1fr", gap:8,
        paddingTop:8, borderTop:"1px solid var(--border-soft)",
      }}>
        <PopMicroStat label="EXPERIMENTS" value={p.variants}/>
        <PopMicroStat label="KEPT"        value={p.kept} accent={p.kept > 0}/>
        <PopMicroStat label="MODEL"       value={p.model.split(" · ")[1] || p.model} mono/>
      </div>

      {/* Mini regime strip */}
      <div style={{display:"flex", gap:3}}>
        {AR_REGIMES.map((r, i) => {
          // Fake some completion ratio per lineage
          const filled = (i + (p.lineage.length % 3)) % 4 === 0 ?
                          0.3 : (i + 1) / 5;
          return (
            <div key={r.id} style={{
              flex:1, height:4, borderRadius:2, position:"relative",
              background:"var(--surface-elev)", overflow:"hidden",
            }}>
              <div style={{
                width:`${filled * 100}%`, height:"100%",
                background: p.status === "paused" ? "var(--text-4)" : statusCol,
              }}/>
            </div>
          );
        })}
      </div>
    </div>
  );
};

const PopMicroStat = ({ label, value, accent = false, mono = false }) => (
  <div>
    <div className="ulabel" style={{fontSize:8.5, letterSpacing:"0.16em"}}>{label}</div>
    <div className={mono ? "mono" : "mono"} style={{
      fontSize: mono ? 11 : 14, fontWeight:600, marginTop:3, lineHeight:1,
      color: accent ? "var(--gold)" : "var(--text)",
    }}>{value}</div>
  </div>
);

// ── Recent cycles table ──
const RecentCyclesTable = () => (
  <>
    {/* Header */}
    <div style={{
      display:"grid",
      gridTemplateColumns:"140px 1fr 90px 80px 90px 110px 110px 90px",
      gap:12, alignItems:"center",
      padding:"10px 16px", borderBottom:"1px solid var(--border-soft)",
    }}>
      {["Cycle ID","Lineage · parent","Experiments","Gate ✓","Kept","Top Δ-Sharpe","Tokens · $","When"].map((h, i) => (
        <div key={i} className="ulabel" style={{
          fontSize:9, letterSpacing:"0.2em", fontWeight:600,
          textAlign: i >= 2 && i <= 5 ? "right" : "left",
        }}>{h}</div>
      ))}
    </div>
    {/* Rows */}
    {AR_RECENT_CYCLES.map((c, i) => (
      <div key={c.id} style={{
        display:"grid",
        gridTemplateColumns:"140px 1fr 90px 80px 90px 110px 110px 90px",
        gap:12, alignItems:"center",
        padding:"11px 16px",
        borderBottom: i < AR_RECENT_CYCLES.length - 1 ? "1px solid var(--border-soft)" : "none",
        cursor:"pointer",
      }}>
        <span className="mono" style={{fontSize:11.5, color:"var(--text)"}}>{c.id}</span>
        <div style={{display:"flex", alignItems:"center", gap:7}}>
          <span className="mono" style={{fontSize:12.5, color:"var(--text)", fontWeight:600}}>{c.lineage}</span>
          <span style={{color:"var(--text-4)"}}>·</span>
          <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>{c.parent}</span>
        </div>
        <span className="mono" style={{fontSize:12, color:"var(--text-2)", textAlign:"right"}}>{c.variants}</span>
        <span className="mono" style={{fontSize:12, color:"var(--text)", textAlign:"right"}}>
          {c.gate}<span style={{color:"var(--text-4)"}}>/{c.variants}</span>
        </span>
        <span className="mono" style={{
          fontSize:12, fontWeight: c.kept > 0 ? 600 : 400,
          color: c.kept > 0 ? "var(--gold)" : "var(--text-3)", textAlign:"right",
        }}>{c.kept}</span>
        <span style={{textAlign:"right"}}><DeltaSharpeCell value={c.deltaTop}/></span>
        <span style={{textAlign:"right", display:"flex", flexDirection:"column", alignItems:"flex-end", gap:1}}>
          <span className="mono" style={{fontSize:11.5, color:"var(--text-2)"}}>{c.costUSD}</span>
          <span className="mono" style={{fontSize:9.5, color:"var(--text-4)"}}>{c.tokens}</span>
        </span>
        <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>{c.when.split(" · ")[0]}</span>
      </div>
    ))}
  </>
);

window.ARHome = ARHome;
window.ARSectionHeader = SectionHeader;
window.ARSmallStat = SmallStat;
window.ARLegendDot = LegendDot;
window.ARHeatCell = HeatCell;
