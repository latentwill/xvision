// Autoresearch · cycle detail — /research/cycle/<id>
// One night's run: hero + eval matrix + variants table + attester activity.

const TallFrame = ({ children }) => (
  <div style={{
    background:"#000", width:"100%", height:"100%", overflow:"hidden",
    display:"grid", gridTemplateColumns:"200px 1fr", position:"relative",
  }}>{children}</div>
);

// Bring in SectionHeader from home
const SectionHeader = window.ARSectionHeader;

// Eval matrix fixture — variant × regime → {sharpe, deltaSharpe, return, status}
// Derived from AR_CURRENT_CYCLE, with synthetic per-cell numbers that are
// internally consistent with each variant's overall verdict.
const buildEvalMatrix = () => {
  const matrix = {};
  AR_CURRENT_CYCLE.variants.forEach((v) => {
    matrix[v.id] = {};
    AR_REGIMES.forEach((r, ri) => {
      if (v.status === "queued") {
        matrix[v.id][r.id] = { status:"queued" };
        return;
      }
      if (v.status === "running") {
        const idx = parseInt(v.summary.match(/(\d) of 5/)?.[1] || "0", 10);
        if (ri < idx)      matrix[v.id][r.id] = synthCell(v, r, ri);
        else if (ri === idx) matrix[v.id][r.id] = { status:"running" };
        else               matrix[v.id][r.id] = { status:"queued" };
        return;
      }
      if (v.status === "failed") {
        if (ri === 3) matrix[v.id][r.id] = { status:"failed" };
        else          matrix[v.id][r.id] = synthCell(v, r, ri);
        return;
      }
      matrix[v.id][r.id] = synthCell(v, r, ri);
    });
  });
  return matrix;
};

function synthCell(v, r, ri) {
  // Spread the variant's overall deltaSharpe across regimes with seeded jitter
  // so each row tells a coherent story.
  const base = v.deltaSharpe ?? 0;
  const seedH = (v.id.charCodeAt(0) + r.id.charCodeAt(0) + ri * 7) % 100;
  const jitter = (seedH / 100 - 0.5) * 0.4;
  // Bear/shock regimes get extra penalty if overall gate failed
  let cellDelta = base + jitter;
  if (v.gate === "FAIL" && (r.kind === "bear" || r.kind === "shock")) cellDelta -= 0.3;
  if (v.gate === "PASS" && r.kind === "shock") cellDelta = Math.max(cellDelta, +0.04);
  const sharpe = v.parent ? 1.31 + cellDelta : 1.0 + cellDelta;
  return {
    status:"done",
    sharpe: sharpe,
    deltaSharpe: cellDelta,
    ret: cellDelta * 0.12,  // approximate return mapping
  };
}

const EVAL_MATRIX = buildEvalMatrix();

// =============================================================================
//  PAGE
// =============================================================================

const ARCycle = () => {
  const cyc = AR_CURRENT_CYCLE;
  return (
    <TallFrame>
      <SideNav active="optimizer" marketplaceVisible={true} optimizerVisible={true}/>
      <main style={{display:"flex", flexDirection:"column", minWidth:0, overflow:"hidden"}}>
        <TopStatus breadcrumb={[
          { text:"OPTIMIZER" },
          { text:"cycle" },
          { text:cyc.id, mono:true },
        ]}/>

        <div style={{flex:1, minHeight:0, overflow:"auto"}}>
          <CycleHero cyc={cyc}/>
          <AntiOverfitGate cyc={cyc}/>
          <EvalMatrixCard cyc={cyc}/>
          <VariantsTable cyc={cyc}/>
          <AttesterActivity/>
          <SummaryPreview cyc={cyc}/>
        </div>
      </main>
    </TallFrame>
  );
};

// ── Hero ──
const CycleHero = ({ cyc }) => {
  const done = cyc.variants.filter(v => v.status === "done").length;
  const running = cyc.variants.filter(v => v.status === "running").length;
  const failed = cyc.variants.filter(v => v.status === "failed").length;
  const kept = cyc.variants.filter(v => v.kept).length;
  const passed = cyc.variants.filter(v => v.gate === "PASS").length;
  const warns  = cyc.variants.filter(v => v.gate === "WARN").length;
  const fails  = cyc.variants.filter(v => v.gate === "FAIL").length;
  const topDelta = Math.max(...cyc.variants.filter(v => v.deltaSharpe !== null).map(v => v.deltaSharpe));

  return (
    <div style={{
      padding:"22px 28px 20px",
      borderBottom:"1px solid var(--border)",
      display:"grid", gridTemplateColumns:"auto 1fr 240px", gap:28, alignItems:"start",
    }}>
      {/* left: parent + dial */}
      <div style={{display:"flex", alignItems:"center", gap:18}}>
        <div style={{position:"relative"}}>
          <GenArt seed={cyc.parentSeed} size={96}
            style={{borderRadius:7, border:"1px solid var(--border)"}}/>
          {/* breeding badge stacked */}
          <div style={{
            position:"absolute", bottom:-6, right:-6, padding:"3px 7px", borderRadius:3,
            background:"rgba(0,0,0,0.7)", backdropFilter:"blur(4px)",
            border:"1px solid var(--gold-soft)",
          }}>
            <span className="mono" style={{
              fontSize:9.5, color:"var(--gold)", letterSpacing:"0.16em", fontWeight:600,
            }}>BREEDING</span>
          </div>
        </div>
        <div>
          <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.22em"}}>CYCLE · BREEDING FROM</div>
          <h1 style={{
            margin:"6px 0 0", fontSize:30, fontWeight:600, letterSpacing:"-0.03em",
            fontFamily:"'Geist Mono', monospace", lineHeight:1,
          }}>{cyc.parent}</h1>
          <div className="mono" style={{fontSize:12, color:"var(--text-2)", marginTop:8}}>
            parent sharpe <span style={{color:"var(--text)"}}>{cyc.parentSharpe.toFixed(2)}</span>
            <span style={{margin:"0 8px", color:"var(--text-4)"}}>·</span>
            cycle id <span style={{color:"var(--text)"}}>{cyc.id}</span>
          </div>
          <div className="mono" style={{fontSize:11, color:"var(--text-3)", marginTop:5}}>
            started <span style={{color:"var(--text-2)"}}>{cyc.startedAt}</span>
            <span style={{margin:"0 6px", color:"var(--text-4)"}}>·</span>
            eta <span style={{color:"var(--gold)"}}>{cyc.expectedEndAt}</span>
          </div>
        </div>
      </div>

      {/* middle: progress + counters */}
      <div style={{display:"flex", flexDirection:"column", gap:14}}>
        <div style={{display:"flex", alignItems:"center", gap:12}}>
          <CycleDial progress={cyc.progress} size={56} stroke={5}/>
          <div style={{flex:1, minWidth:0}}>
            <div style={{display:"flex", alignItems:"center", gap:10}}>
              <ARStatusPill status="running"/>
              <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>
                elapsed <span style={{color:"var(--text)"}}>{cyc.elapsed}</span>
                <span style={{margin:"0 6px", color:"var(--text-4)"}}>·</span>
                eta <span style={{color:"var(--text)"}}>{cyc.remaining}</span>
              </span>
            </div>
            <div style={{marginTop:8}}>
              <ARProgressBar value={cyc.evalsDone} total={cyc.evalsTotal} height={5}/>
            </div>
            <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginTop:6}}>
              {cyc.evalsDone}/{cyc.evalsTotal} evals · {cyc.evalsFailed} retried
            </div>
          </div>
        </div>
        <div style={{
          display:"grid", gridTemplateColumns:"repeat(5, 1fr)", gap:10,
          paddingTop:14, borderTop:"1px solid var(--border-soft)",
        }}>
          <HeroCounter label="EXPERIMENTS"  value={cyc.variants.length} sub={`${done} done · ${running} testing · ${failed} failed`}/>
          <HeroCounter label="GATE ✓"      value={passed}              sub={`${warns} suspect · ${fails} dropped`}
            tone={passed > 0 ? "gold" : "neutral"}/>
          <HeroCounter label="KEPT"        value={kept}              sub={`${cyc.variants.length - kept} not kept`}
            tone="gold"/>
          <HeroCounter label="TOP Δ"       value={`+${topDelta.toFixed(2)}`} sub="vs parent" tone="gold"/>
          <HeroCounter label="$ SPEND"     value={cyc.costUSD}         sub={`${cyc.tokensSpent} tokens`}/>
        </div>
      </div>

      {/* right: actions */}
      <div style={{display:"flex", flexDirection:"column", gap:8}}>
        <Btn variant="primary" icon="bolt" style={{justifyContent:"center"}}>Sign off summary now</Btn>
        <Btn variant="ghost" icon="ext"    style={{justifyContent:"center"}}>Open in Marketplace</Btn>
        <div style={{display:"grid", gridTemplateColumns:"1fr 1fr", gap:6, marginTop:4}}>
          <Btn variant="ghost" dense style={{justifyContent:"center"}}>Pause</Btn>
          <Btn variant="ghost" dense style={{justifyContent:"center"}}>Skip queued</Btn>
        </div>
        <div style={{
          marginTop:6, padding:"10px 11px", border:"1px dashed var(--border-strong)", borderRadius:5,
        }}>
          <div className="ulabel" style={{fontSize:9, letterSpacing:"0.2em", marginBottom:5}}>OPERATOR</div>
          <div className="mono" style={{fontSize:10.5, color:"var(--text-2)", lineHeight:1.5}}>
            intervene before <span style={{color:"var(--gold)"}}>06:14 sign-off</span>.
            <br/>after that, the evening summary is locked locally.
            <br/><span style={{color:"var(--text-3)"}}>publishing to chain is a separate Marketplace step.</span>
          </div>
        </div>
      </div>
    </div>
  );
};

const HeroCounter = ({ label, value, sub, tone = "neutral" }) => {
  const valueCol = tone === "gold" ? "var(--gold)" : "var(--text)";
  return (
    <div>
      <div className="ulabel" style={{fontSize:9, letterSpacing:"0.2em"}}>{label}</div>
      <div className="mono" style={{
        fontSize:22, fontWeight:600, marginTop:5, color:valueCol, letterSpacing:"-0.02em", lineHeight:1,
      }}>{value}</div>
      <div className="mono" style={{fontSize:10, color:"var(--text-3)", marginTop:4}}>{sub}</div>
    </div>
  );
};

// ── Anti-overfit gate explainer ──
const AntiOverfitGate = ({ cyc }) => {
  const passed = cyc.variants.filter(v => v.gate === "PASS").length;
  const warns  = cyc.variants.filter(v => v.gate === "WARN").length;
  const fails  = cyc.variants.filter(v => v.gate === "FAIL").length;
  const pending = cyc.variants.length - passed - warns - fails - cyc.variants.filter(v => v.gate === "X").length;
  return (
    <div style={{padding:"18px 28px 0"}}>
      <div style={{
        border:"1px solid var(--border)", borderRadius:6,
        background:"var(--surface-card)",
        padding:"14px 18px",
        display:"grid", gridTemplateColumns:"1fr 1fr 1fr 1fr", gap:18, alignItems:"center",
      }}>
        <div>
          <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.2em"}}>ANTI-OVERFIT GATE</div>
          <div style={{fontSize:13, color:"var(--text)", marginTop:6, fontWeight:500, lineHeight:1.4}}>
            Experiment must show <span style={{color:"var(--gold)"}}>positive Δ-Sharpe</span> in
            <span style={{color:"var(--gold)"}}> ≥1 bull</span> regime
            <span style={{color:"var(--text-3)"}}> AND</span>
            <span style={{color:"var(--gold)"}}> ≥1 bear or shock</span> regime to be kept.
          </div>
          <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginTop:6}}>
            v1 gate · pre-committed metric Δ-Sharpe vs parent
          </div>
        </div>
        <GateBucket label="KEPT · ready to sign off" count={passed} total={cyc.variants.length} tone="gold"/>
        <GateBucket label="SUSPECT · held for review" count={warns}  total={cyc.variants.length} tone="warn"/>
        <GateBucket label="DROPPED · check failed"    count={fails}  total={cyc.variants.length} tone="danger"/>
      </div>
    </div>
  );
};

const GateBucket = ({ label, count, total, tone }) => {
  const map = {
    gold:   { fg:"var(--gold)",   bd:"var(--gold-soft)" },
    warn:   { fg:"var(--warn)",   bd:"rgba(255,176,32,0.40)" },
    danger: { fg:"var(--danger)", bd:"rgba(255,77,77,0.40)" },
  };
  const c = map[tone];
  return (
    <div style={{
      padding:"10px 12px", border:`1px solid ${c.bd}`, borderRadius:5,
      background:`color-mix(in oklab, ${c.fg} 6%, transparent)`,
    }}>
      <div className="ulabel" style={{fontSize:8.5, letterSpacing:"0.18em", color:c.fg}}>{label}</div>
      <div style={{display:"flex", alignItems:"baseline", gap:6, marginTop:5}}>
        <span className="mono" style={{
          fontSize:22, fontWeight:600, color:c.fg, lineHeight:1, letterSpacing:"-0.02em",
        }}>{count}</span>
        <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>/ {total}</span>
      </div>
    </div>
  );
};

// ── Full eval matrix ──
const EvalMatrixCard = ({ cyc }) => {
  return (
    <div style={{padding:"18px 28px 0"}}>
      <SectionHeader title="Eval matrix · experiments × regimes"
        sub="Δ-Sharpe vs parent · click a cell to open the flight recorder · click a row for the experiment inspector"
        right={<>
          <Btn variant="ghost" dense icon="ext">Export CSV</Btn>
          <Btn variant="chip" dense>Δ-Sharpe</Btn>
          <Btn variant="ghost" dense>Sharpe</Btn>
          <Btn variant="ghost" dense>Return</Btn>
        </>}/>

      <div style={{
        marginTop:14, border:"1px solid var(--border)", borderRadius:6,
        background:"var(--surface-card)",
      }}>
        {/* header */}
        <div style={{
          display:"grid",
          gridTemplateColumns:"110px 130px repeat(5, 1fr) 80px 90px",
          gap:8, alignItems:"center",
          padding:"10px 16px", borderBottom:"1px solid var(--border)",
        }}>
          <div className="ulabel" style={{fontSize:9, letterSpacing:"0.2em"}}>Experiment</div>
          <div className="ulabel" style={{fontSize:9, letterSpacing:"0.2em"}}>Kind</div>
          {AR_REGIMES.map((r) => (
            <div key={r.id} style={{
              display:"flex", alignItems:"center", gap:5, justifyContent:"center",
            }}>
              <RegimeIcon kind={r.kind} size={11} color={REGIME_KIND_COLOR[r.kind]}/>
              <span className="mono" style={{
                fontSize:10, color:"var(--text-2)", letterSpacing:"0.04em",
              }}>{r.label}</span>
            </div>
          ))}
          <div className="ulabel" style={{fontSize:9, letterSpacing:"0.2em", textAlign:"right"}}>Sharpe</div>
          <div className="ulabel" style={{fontSize:9, letterSpacing:"0.2em", textAlign:"center"}}>Gate</div>
        </div>

        {cyc.variants.map((v, vi) => (
          <div key={v.id} style={{
            display:"grid",
            gridTemplateColumns:"110px 130px repeat(5, 1fr) 80px 90px",
            gap:8, alignItems:"center",
            padding:"10px 16px",
            borderBottom: vi < cyc.variants.length - 1 ? "1px solid var(--border-soft)" : "none",
            background: v.kept ? "rgba(0,230,118,0.04)" : "transparent",
            cursor:"pointer",
          }}>
            <div style={{display:"flex", alignItems:"center", gap:7}}>
              <span style={{
                width:6, height:6, borderRadius:"50%",
                background: v.kept ? "var(--gold)" : v.gate === "FAIL" ? "var(--danger)" : "var(--text-4)",
              }}/>
              <span className="mono" style={{
                fontSize:11.5, color:"var(--text)", fontWeight:600,
              }}>{v.id}</span>
            </div>
            <div><ExperimentPill kind={v.experiment}/></div>
            {AR_REGIMES.map((r) => {
              const cell = EVAL_MATRIX[v.id][r.id];
              return <DeltaCell key={r.id} cell={cell}/>;
            })}
            <span className="mono" style={{
              fontSize:12, color: v.sharpe ? "var(--text)" : "var(--text-4)",
              textAlign:"right", fontWeight:600,
            }}>
              {v.sharpe ? v.sharpe.toFixed(2) : "—"}
            </span>
            <div style={{display:"flex", justifyContent:"center"}}>
              <GateBadge verdict={v.gate} size="sm"/>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

const DeltaCell = ({ cell }) => {
  if (cell.status === "queued") {
    return (
      <div style={{
        height:30, border:"1px solid var(--border-soft)", borderRadius:3,
        background:"var(--surface-elev)",
      }}/>
    );
  }
  if (cell.status === "running") {
    return (
      <div style={{
        height:30, border:"1px solid rgba(95,168,255,0.40)", borderRadius:3,
        background:"rgba(95,168,255,0.12)",
        display:"flex", alignItems:"center", justifyContent:"center", position:"relative", overflow:"hidden",
      }}>
        <span className="mono" style={{
          fontSize:9.5, color:"var(--info)", letterSpacing:"0.14em", fontWeight:600,
        }}>RUN…</span>
        <div className="bar-flow" style={{
          position:"absolute", inset:0,
          background:"linear-gradient(90deg, transparent, rgba(95,168,255,0.20), transparent)",
        }}/>
      </div>
    );
  }
  if (cell.status === "failed") {
    return (
      <div style={{
        height:30, border:"1px solid rgba(255,77,77,0.40)", borderRadius:3,
        background:"rgba(255,77,77,0.10)",
        display:"flex", alignItems:"center", justifyContent:"center",
      }}>
        <span className="mono" style={{
          fontSize:9.5, color:"var(--danger)", letterSpacing:"0.14em", fontWeight:600,
        }}>RETRY×2</span>
      </div>
    );
  }
  // done — show Δ-Sharpe with intensity background
  const d = cell.deltaSharpe;
  const positive = d >= 0;
  const intensity = Math.min(Math.abs(d) / 0.5, 1);
  const bg = positive
    ? `rgba(0,230,118,${0.06 + intensity * 0.20})`
    : `rgba(255,77,77,${0.06 + intensity * 0.20})`;
  const bd = positive
    ? `rgba(0,230,118,${0.20 + intensity * 0.40})`
    : `rgba(255,77,77,${0.20 + intensity * 0.40})`;
  return (
    <div style={{
      height:30, padding:"0 8px", borderRadius:3,
      border:`1px solid ${bd}`, background:bg,
      display:"flex", alignItems:"center", justifyContent:"space-between", gap:6,
    }}>
      <span className="mono" style={{
        fontSize:11, color: positive ? "var(--gold)" : "var(--danger)", fontWeight:600,
      }}>{positive ? "+" : ""}{d.toFixed(2)}</span>
      <span className="mono" style={{
        fontSize:9.5, color:"var(--text-3)",
      }}>S {cell.sharpe.toFixed(2)}</span>
    </div>
  );
};

// ── Variants list (the rationale column) ──
const VariantsTable = ({ cyc }) => (
  <div style={{padding:"22px 28px 0"}}>
    <SectionHeader title="Experiments this cycle"
      sub="what the optimizer tried · why it tried it · what was kept"
      right={<>
        <Btn variant="ghost" dense>Hide queued</Btn>
        <Btn variant="ghost" dense icon="ext">Open lineage tree</Btn>
      </>}/>
    <div style={{
      marginTop:14, border:"1px solid var(--border)", borderRadius:6,
    }}>
      {/* header */}
      <div style={{
        display:"grid",
        gridTemplateColumns:"48px 90px 130px 1fr 180px 110px 70px",
        gap:12, alignItems:"center",
        padding:"10px 16px", borderBottom:"1px solid var(--border)",
      }}>
        {["","Experiment","Kind","Why · what changed","Attesters","Δ vs parent","Kept"].map((h, i) => (
          <div key={i} className="ulabel" style={{
            fontSize:9, letterSpacing:"0.2em",
            textAlign: i === 5 ? "right" : i === 6 ? "center" : "left",
          }}>{h}</div>
        ))}
      </div>
      {cyc.variants.map((v, vi) => <VariantRow key={v.id} v={v} index={vi} total={cyc.variants.length}/>)}
    </div>
  </div>
);

const VariantRow = ({ v, index, total }) => {
  const muted = v.status === "queued" || v.status === "failed";
  return (
    <div style={{
      display:"grid",
      gridTemplateColumns:"48px 90px 130px 1fr 180px 110px 70px",
      gap:12, alignItems:"center",
      padding:"12px 16px",
      borderBottom: index < total - 1 ? "1px solid var(--border-soft)" : "none",
      background: v.kept ? "rgba(0,230,118,0.04)" : "transparent",
      cursor:"pointer", opacity: muted ? 0.7 : 1,
    }}>
      <GenArt seed={v.seed} size={36} style={{borderRadius:4, border:"1px solid var(--border)"}}/>
      <div style={{display:"flex", flexDirection:"column", gap:4}}>
        <span className="mono" style={{fontSize:12, color:"var(--text)", fontWeight:600}}>{v.id}</span>
        <ARStatusPill status={v.status}/>
      </div>
      <ExperimentPill kind={v.experiment}/>
      <span style={{fontSize:12, color:"var(--text-2)", lineHeight:1.45}}>{v.summary}</span>
      <AttesterStrip atts={v.attestations} status={v.status}/>
      <div style={{textAlign:"right"}}>
        {v.deltaSharpe !== null ? <DeltaSharpeCell value={v.deltaSharpe} size="lg"/> :
          <span className="mono" style={{fontSize:12, color:"var(--text-4)"}}>—</span>}
      </div>
      <div style={{display:"flex", justifyContent:"center"}}>
        {v.kept ? (
          <span title="Kept · pending sign-off at 06:14" style={{
            display:"inline-flex", alignItems:"center", justifyContent:"center",
            width:22, height:22, borderRadius:"50%",
            background:"var(--gold-bg)", border:"1px solid var(--gold)",
          }}>
            <Icon name="check" size={11} color="var(--gold)" sw={2.5}/>
          </span>
        ) : v.gate === "FAIL" || v.gate === "X" ? (
          <span style={{
            display:"inline-flex", alignItems:"center", justifyContent:"center",
            width:22, height:22, borderRadius:"50%",
            border:"1px dashed var(--border-strong)",
          }}>
            <span style={{fontSize:11, color:"var(--text-4)"}}>—</span>
          </span>
        ) : (
          <span className="mono" style={{fontSize:10, color:"var(--text-3)", letterSpacing:"0.12em"}}>HOLD</span>
        )}
      </div>
    </div>
  );
};

const AttesterStrip = ({ atts, status }) => {
  if (status === "queued" || status === "running") {
    return <span className="mono" style={{fontSize:10.5, color:"var(--text-4)", letterSpacing:"0.08em"}}>
      awaiting evals…
    </span>;
  }
  const items = [];
  if (atts.endorse) items.push(["ENDORSE", "var(--gold)", atts.endorse]);
  if (atts.question) items.push(["QUESTION", "var(--warn)", atts.question]);
  if (atts.reject) items.push(["REJECT", "var(--danger)", atts.reject]);
  if (items.length === 0) {
    return <span className="mono" style={{fontSize:10.5, color:"var(--text-4)"}}>—</span>;
  }
  return (
    <div style={{display:"flex", flexWrap:"wrap", gap:5}}>
      {items.map(([label, col, n]) => (
        <span key={label} style={{
          display:"inline-flex", alignItems:"center", gap:5,
          padding:"2px 7px", borderRadius:3,
          border:`1px solid ${col}`, color:col,
        }}>
          <span style={{width:4, height:4, borderRadius:"50%", background:col}}/>
          <span className="mono" style={{
            fontSize:9.5, letterSpacing:"0.14em", fontWeight:600,
          }}>{label}</span>
          <span className="mono" style={{fontSize:10}}>{n}</span>
        </span>
      ))}
    </div>
  );
};

// ── Attester activity ──
const ATTESTER_EVENTS = [
  { t:"04:11", attester:"regime-verifier", token:"#0007", verdict:"ENDORSE", target:"v3.1.g",
    note:"all 5 regimes pass regime-tag commitment" },
  { t:"04:09", attester:"diversity-check", token:"#0008", verdict:"ENDORSE", target:"v3.1.g",
    note:"variety score 0.241 vs parent (threshold ≥ 0.18)" },
  { t:"03:48", attester:"regime-verifier", token:"#0007", verdict:"ENDORSE", target:"v3.1.b",
    note:"regime claims consistent with trace" },
  { t:"03:31", attester:"diversity-check", token:"#0008", verdict:"QUESTION", target:"v3.1.e",
    note:"variety score 0.171 — below threshold" },
  { t:"03:14", attester:"regime-verifier", token:"#0007", verdict:"ENDORSE", target:"v3.1.a",
    note:"clean regime tagging across all bars" },
  { t:"03:02", attester:"regime-verifier", token:"#0007", verdict:"REJECT",   target:"v3.1.f",
    note:"regime-detect removed · cannot validate without classifier" },
  { t:"02:58", attester:"diversity-check", token:"#0008", verdict:"REJECT",   target:"v3.1.f",
    note:"agent-remove experiment breaks variety lineage" },
  { t:"02:41", attester:"diversity-check", token:"#0008", verdict:"QUESTION", target:"v3.1.h",
    note:"model-swap blurs variety similarity — manual review" },
];

const AttesterActivity = () => (
  <div style={{padding:"22px 28px 0"}}>
    <SectionHeader title="Attester activity"
      sub="2 local attester agents · sign sign-off receipts as experiments finish · publishes to chain only via Marketplace opt-in"
      right={<>
        <Btn variant="ghost" dense icon="shield">Manage attesters</Btn>
        <Btn variant="ghost" dense icon="ext">Export receipts</Btn>
      </>}/>
    <div style={{
      marginTop:14, border:"1px solid var(--border)", borderRadius:6,
      background:"var(--surface-card)",
    }}>
      {/* attester avatars row */}
      <div style={{
        padding:"12px 16px", borderBottom:"1px solid var(--border)",
        display:"grid", gridTemplateColumns:"1fr 1fr", gap:14,
      }}>
        <AttesterCard name="regime-verifier" token="#0007"
          blurb="verifies regime claim against trace"
          stats={[["endorse", 4, "var(--gold)"], ["question", 0, "var(--warn)"], ["reject", 1, "var(--danger)"]]}/>
        <AttesterCard name="diversity-check" token="#0008"
          blurb="confirms experiment adds variety"
          stats={[["endorse", 3, "var(--gold)"], ["question", 2, "var(--warn)"], ["reject", 1, "var(--danger)"]]}/>
      </div>
      {/* event log */}
      <div>
        {/* header */}
        <div style={{
          display:"grid",
          gridTemplateColumns:"70px 110px 100px 100px 80px 1fr",
          gap:12, alignItems:"center",
          padding:"9px 16px", borderBottom:"1px solid var(--border-soft)",
        }}>
          {["When","Attester","ID","Verdict","Target","Note"].map((h, i) => (
            <div key={i} className="ulabel" style={{fontSize:9, letterSpacing:"0.2em"}}>{h}</div>
          ))}
        </div>
        {ATTESTER_EVENTS.map((e, i) => {
          const verdictCol = e.verdict === "ENDORSE" ? "var(--gold)" :
                             e.verdict === "QUESTION" ? "var(--warn)" : "var(--danger)";
          return (
            <div key={i} style={{
              display:"grid",
              gridTemplateColumns:"70px 110px 100px 100px 80px 1fr",
              gap:12, alignItems:"center",
              padding:"9px 16px",
              borderBottom: i < ATTESTER_EVENTS.length - 1 ? "1px solid var(--border-soft)" : "none",
            }}>
              <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>{e.t}</span>
              <span className="mono" style={{fontSize:11.5, color:"var(--text-2)"}}>{e.attester}</span>
              <span className="mono" style={{fontSize:11, color:"var(--gold)"}}>{e.token}</span>
              <span style={{
                display:"inline-flex", alignItems:"center", gap:5,
                padding:"2px 7px", borderRadius:3,
                border:`1px solid ${verdictCol}`, color:verdictCol, width:"fit-content",
              }}>
                <span style={{width:4, height:4, borderRadius:"50%", background:verdictCol}}/>
                <span className="mono" style={{
                  fontSize:9.5, letterSpacing:"0.14em", fontWeight:600,
                }}>{e.verdict}</span>
              </span>
              <span className="mono" style={{fontSize:11.5, color:"var(--text)"}}>{e.target}</span>
              <span style={{fontSize:11.5, color:"var(--text-2)"}}>{e.note}</span>
            </div>
          );
        })}
      </div>
    </div>
  </div>
);

const AttesterCard = ({ name, token, blurb, stats }) => (
  <div style={{
    padding:"10px 12px", border:"1px solid var(--border)", borderRadius:5,
    background:"transparent", display:"flex", alignItems:"center", gap:12,
  }}>
    <div style={{
      width:32, height:32, borderRadius:5,
      background:"var(--gold-bg)", border:"1px solid var(--gold-soft)",
      display:"flex", alignItems:"center", justifyContent:"center", flexShrink:0,
    }}>
      <Icon name="shield" size={14} color="var(--gold)"/>
    </div>
    <div style={{flex:1, minWidth:0}}>
      <div style={{display:"flex", alignItems:"center", gap:8}}>
        <span style={{fontSize:13, color:"var(--text)", fontWeight:600}}>{name}</span>
        <span className="mono" style={{fontSize:10.5, color:"var(--gold)"}}>local</span>
      </div>
      <div style={{fontSize:11, color:"var(--text-3)", marginTop:3}}>{blurb}</div>
    </div>
    <div style={{display:"flex", gap:10}}>
      {stats.map(([k, v, col]) => (
        <div key={k} style={{textAlign:"center"}}>
          <div className="ulabel" style={{fontSize:8.5, letterSpacing:"0.14em"}}>{k}</div>
          <div className="mono" style={{fontSize:13, fontWeight:600, color:col, marginTop:2}}>{v}</div>
        </div>
      ))}
    </div>
  </div>
);

// ── Evening summary preview ──
const SummaryPreview = ({ cyc }) => {
  const kept = cyc.variants.filter(v => v.kept);
  return (
    <div style={{padding:"22px 28px 28px"}}>
      <SectionHeader title="Evening summary preview"
        sub="06:14 sign-off · committed locally · stays on disk until you publish to Marketplace"/>
      <div style={{
        marginTop:14,
        display:"grid", gridTemplateColumns:"1fr 320px", gap:14,
      }}>
        {/* receipt mock */}
        <div style={{
          border:"1px solid var(--border)", borderRadius:6, background:"var(--surface-card)",
          padding:"14px 16px", fontFamily:"'Geist Mono', monospace",
        }}>
          <div style={{display:"flex", alignItems:"center", gap:8, marginBottom:10}}>
            <Icon name="diamond" size={13} color="var(--gold)"/>
            <span className="mono" style={{fontSize:11, color:"var(--text-2)", letterSpacing:"0.14em"}}>
              EVENING SUMMARY · PREVIEW
            </span>
            <span style={{marginLeft:"auto"}}>
              <StatusPill tone="info">LOCAL · UNPUBLISHED</StatusPill>
            </span>
          </div>
          <ReceiptRow k="cycle_id"          v={cyc.id}/>
          <ReceiptRow k="lineage"           v={cyc.lineage}/>
          <ReceiptRow k="parent_fingerprint" v="7f2b1ad…91c4"/>
          <ReceiptRow k="started_at"        v={cyc.startedAt}/>
          <ReceiptRow k="signed_at"         v="2026-05-27 06:14:00"  tone="gold"/>
          <ReceiptRow k="kept[]"            v={`[ ${kept.map(v => v.id).join(", ")} ]`} tone="gold"/>
          <ReceiptRow k="gate_summary"      v={`kept=${cyc.variants.filter(v=>v.gate==="PASS").length} suspect=${cyc.variants.filter(v=>v.gate==="WARN").length} dropped=${cyc.variants.filter(v=>v.gate==="FAIL").length}`}/>
          <ReceiptRow k="attesters[]"       v="[ regime-verifier, diversity-check ]"/>
          <ReceiptRow k="your_signature"    v="signed locally · 7f2b1ad…91c4"/>
          <ReceiptRow k="bundle_fingerprint" v="c3e9a4f…7a02"/>
          <ReceiptRow k="storage"           v="~/.xvn/cycles/cyc-01N8R2K9/" tone="info"/>
        </div>

        {/* kept experiments gen-art */}
        <div style={{
          border:"1px solid var(--gold-soft)", borderRadius:6,
          background:"linear-gradient(180deg, rgba(0,230,118,0.04), rgba(0,230,118,0.01))",
          padding:"14px 14px",
        }}>
          <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.2em", marginBottom:10}}>KEPT TONIGHT</div>
          <div style={{display:"flex", flexDirection:"column", gap:8}}>
            {kept.map((v) => (
              <div key={v.id} style={{
                display:"flex", alignItems:"center", gap:10,
                padding:"8px 9px", borderRadius:4, background:"rgba(0,0,0,0.3)",
              }}>
                <GenArt seed={v.seed} size={32} style={{borderRadius:3, border:"1px solid rgba(0,0,0,0.5)"}}/>
                <div style={{flex:1, minWidth:0}}>
                  <div style={{display:"flex", alignItems:"center", gap:6}}>
                    <span className="mono" style={{fontSize:11.5, color:"var(--text)", fontWeight:600}}>{v.id}</span>
                  </div>
                  <DeltaSharpeCell value={v.deltaSharpe}/>
                </div>
                <Icon name="check" size={12} color="var(--gold)" sw={2}/>
              </div>
            ))}
          </div>
          <div style={{
            marginTop:12, paddingTop:10, borderTop:"1px solid rgba(0,230,118,0.18)",
          }}>
            <button style={{
              width:"100%", padding:"8px 12px",
              background:"transparent", color:"var(--gold)", border:"1px solid var(--gold-soft)", borderRadius:4,
              fontFamily:"'Geist', sans-serif", fontSize:12, fontWeight:600,
              letterSpacing:"0.01em", cursor:"pointer",
              display:"flex", alignItems:"center", justifyContent:"center", gap:6,
            }}>
              <Icon name="ext" size={11} color="var(--gold)"/>
              Publish to chain (optional)
            </button>
            <div className="mono" style={{
              fontSize:9.5, color:"var(--text-3)", marginTop:7, lineHeight:1.5, textAlign:"center",
            }}>
              opens Marketplace · adds lineage proof on chain · costs gas
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

const ReceiptRow = ({ k, v, tone = "neutral" }) => {
  const col = tone === "gold" ? "var(--gold)" : tone === "warn" ? "var(--warn)" : "var(--text)";
  return (
    <div style={{
      display:"grid", gridTemplateColumns:"160px 1fr", gap:10,
      padding:"5px 0", borderBottom:"1px solid var(--border-soft)",
    }}>
      <span className="ulabel" style={{fontSize:9.5, letterSpacing:"0.14em"}}>{k}</span>
      <span className="mono" style={{fontSize:11, color:col, wordBreak:"break-all"}}>{v}</span>
    </div>
  );
};

// Bring in SectionHeader (declared at top)

window.ARCycle = ARCycle;
