// Autoresearch · variant inspector — /research/variant/<id>
// Single variant deep-dive: parent diff · per-regime evals · attestations · flight recorder.
// Story: v3.1.g — the top performer this cycle (prompt-tweak, +0.22 Δ, kept).

const TallFrameV = ({ children }) => (
  <div style={{
    background:"#000", width:"100%", height:"100%", overflow:"hidden",
    display:"grid", gridTemplateColumns:"200px 1fr", position:"relative",
  }}>{children}</div>
);

const SectionHeaderV = window.ARSectionHeader;

// The variant we're inspecting (operator-facing: "experiment")
const VARIANT = AR_CURRENT_CYCLE.variants.find(v => v.id === "v3.1.g");
const PARENT  = AR_CURRENT_CYCLE.parent;
const PARENT_SEED = AR_CURRENT_CYCLE.parentSeed;
const PARENT_SHARPE = AR_CURRENT_CYCLE.parentSharpe;

// Per-regime numbers for v3.1.g — handcrafted to support the story
const REGIME_RESULTS = [
  { regime:"bull-q1-25",       sharpe:1.62, delta:+0.31, ret:"+18.4%", dd:"-4.1%", wr:"66%", trades:142 },
  { regime:"chop-q2-25",       sharpe:1.18, delta:+0.21, ret:"+5.4%",  dd:"-3.8%", wr:"54%", trades:118 },
  { regime:"bear-q3-24",       sharpe:1.04, delta:+0.16, ret:"+2.1%",  dd:"-5.2%", wr:"51%", trades:88  },
  { regime:"flash-crash-24-08",sharpe:0.74, delta:+0.08, ret:"-1.4%",  dd:"-7.8%", wr:"42%", trades:14  },
  { regime:"chop-q4-23",       sharpe:1.29, delta:+0.27, ret:"+6.8%",  dd:"-2.9%", wr:"57%", trades:104 },
];

// =============================================================================
//  PAGE
// =============================================================================

const ARVariant = () => {
  return (
    <TallFrameV>
      <SideNav active="optimizer" marketplaceVisible={true} optimizerVisible={true}/>
      <main style={{display:"flex", flexDirection:"column", minWidth:0, overflow:"hidden"}}>
        <TopStatus breadcrumb={[
          { text:"OPTIMIZER" },
          { text:"cycle" },
          { text:AR_CURRENT_CYCLE.id, mono:true },
          { text:"experiment" },
          { text:VARIANT.id, mono:true },
        ]}/>
        <div style={{flex:1, minHeight:0, overflow:"auto"}}>
          <VariantHero/>
          <ParentDiff/>
          <PerRegimeStrip/>
          <FlightRecorder/>
          <AttestationDetail/>
          <DecisionStrip/>
        </div>
      </main>
    </TallFrameV>
  );
};

// ── Hero ──
const VariantHero = () => (
  <div style={{
    padding:"22px 28px 22px",
    borderBottom:"1px solid var(--border)",
    display:"grid", gridTemplateColumns:"260px 1fr 280px", gap:24, alignItems:"start",
  }}>
    {/* lineage strip: parent → variant */}
    <div style={{display:"flex", flexDirection:"column", gap:14, alignItems:"center"}}>
      <div style={{display:"flex", alignItems:"center", gap:12}}>
        <div style={{textAlign:"center"}}>
          <GenArt seed={PARENT_SEED} size={72} style={{borderRadius:5, border:"1px solid var(--border)"}}/>
          <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginTop:6, fontWeight:600}}>
            {PARENT}
          </div>
          <div className="mono" style={{fontSize:10, color:"var(--text-4)", marginTop:2}}>parent</div>
        </div>
        <div style={{
          display:"flex", flexDirection:"column", alignItems:"center", gap:3,
          color:"var(--gold)",
        }}>
          <svg width="36" height="24" viewBox="0 0 36 24" fill="none" stroke="currentColor"
            strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M2 12h28M22 4l8 8-8 8"/>
          </svg>
          <ExperimentPill kind={VARIANT.experiment}/>
        </div>
        <div style={{textAlign:"center"}}>
          <GenArt seed={VARIANT.seed} size={88}
            style={{borderRadius:6, border:"2px solid var(--gold)"}}/>
          <div className="mono" style={{fontSize:11, color:"var(--gold)", marginTop:6, fontWeight:700}}>
            {VARIANT.id}
          </div>
          <div className="mono" style={{fontSize:10, color:"var(--text-3)", marginTop:2}}>experiment</div>
        </div>
      </div>
    </div>

    {/* metrics + verdict */}
    <div style={{minWidth:0}}>
      <div style={{display:"flex", alignItems:"center", gap:10, flexWrap:"wrap"}}>
        <span className="ulabel" style={{fontSize:9.5, letterSpacing:"0.22em"}}>OPTIMIZER · EXPERIMENT</span>
        <span style={{color:"var(--text-4)"}}>·</span>
        <GateBadge verdict="PASS"/>
        <span className="mono" style={{fontSize:10.5, color:"var(--gold)", letterSpacing:"0.14em", fontWeight:600}}>KEPT</span>
        <span style={{color:"var(--text-4)"}}>·</span>
        <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>top performer · this cycle</span>
      </div>
      <h1 style={{
        margin:"10px 0 0", fontSize:30, fontWeight:600, letterSpacing:"-0.03em",
        fontFamily:"'Geist Mono', monospace", lineHeight:1.1,
      }}>{AR_CURRENT_CYCLE.lineage} · {VARIANT.id}</h1>
      <p style={{
        margin:"10px 0 0", fontSize:14.5, color:"var(--text)", lineHeight:1.5,
        maxWidth:560,
      }}>{VARIANT.summary}</p>

      {/* big numbers */}
      <div style={{
        marginTop:18,
        display:"grid", gridTemplateColumns:"auto 1fr 1fr 1fr 1fr", gap:18, alignItems:"end",
      }}>
        <div>
          <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.2em", marginBottom:5}}>Δ-SHARPE vs PARENT</div>
          <div className="mono" style={{
            fontSize:42, fontWeight:600, color:"var(--gold)", letterSpacing:"-0.03em", lineHeight:1,
          }}>+{VARIANT.deltaSharpe.toFixed(2)}</div>
        </div>
        <MetricCell label="Sharpe"    value={VARIANT.sharpe.toFixed(2)}/>
        <MetricCell label="vs parent" value={`${PARENT_SHARPE.toFixed(2)} → ${VARIANT.sharpe.toFixed(2)}`}/>
        <MetricCell label="Kept on"   value="all 5 regimes" tone="gold"/>
        <MetricCell label="Sign-offs" value="2 endorse · 0 ?  · 0 ✗"/>
      </div>
    </div>

    {/* actions */}
    <div style={{display:"flex", flexDirection:"column", gap:8}}>
      <button style={{
        padding:"10px 12px", borderRadius:4,
        background:"var(--gold)", color:"#001A0A", border:"none",
        fontFamily:"'Geist', sans-serif", fontSize:13.5, fontWeight:700,
        cursor:"pointer", letterSpacing:"0.01em",
        display:"flex", alignItems:"center", justifyContent:"center", gap:6,
      }}>
        <Icon name="bolt" size={13} color="#001A0A"/>
        Activate for paper trading
      </button>
      <Btn variant="ghost" icon="ext" style={{justifyContent:"center"}}>
        View Marketplace listing
      </Btn>
      <Btn variant="ghost" icon="branch" style={{justifyContent:"center"}}>
        Fork to manual edit
      </Btn>
      <div style={{display:"grid", gridTemplateColumns:"1fr 1fr", gap:6, marginTop:4}}>
        <Btn variant="ghost" dense style={{justifyContent:"center"}}>Re-eval</Btn>
        <Btn variant="danger" dense style={{justifyContent:"center"}}>Drop</Btn>
      </div>
    </div>
  </div>
);

// MetricCell exists in bc-shared? No, in bc2-lineage. Need local
const MetricCell = ({ label, value, tone = "text" }) => (
  <div>
    <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em", marginBottom:4}}>{label.toUpperCase()}</div>
    <div className="mono" style={{
      fontSize:14, fontWeight:600, lineHeight:1.15,
      color: tone === "gold" ? "var(--gold)" : tone === "warn" ? "var(--warn)" : "var(--text)",
    }}>{value}</div>
  </div>
);

// ── Parent diff — the unique value of this view ──
const DIFF = [
  { section:"system prompt · regime detection",
    before:`When you see 3+ bars with a long upper wick and rising volume, lean cautious.
Flag the bar series as "potential exhaustion" only when wick > 2× body.`,
    after:`When you see 3+ bars with a long upper wick and rising volume, classify the series as one of:
  · trend-continuation (volume ≥ 1.4× 20-bar avg AND close > VWAP)
  · exhaustion         (wick ≥ 2× body AND close < midpoint)
  · ambiguous          (otherwise — return empty TraderArm)
Reject ambiguous bars from Stage 2 entirely.`,
    kind:"changed", lineDelta:"+5 / −2" },
  { section:"threshold · stop-loss",
    before:"stop_loss_pct: 0.012", after:"stop_loss_pct: 0.012", kind:"unchanged", lineDelta:"—" },
  { section:"threshold · take-profit",
    before:"take_profit_pct: 0.024", after:"take_profit_pct: 0.024", kind:"unchanged", lineDelta:"—" },
  { section:"agent topology",
    before:"intern → trader → risk-layer → execution",
    after:"intern → trader → risk-layer → execution",
    kind:"unchanged", lineDelta:"—" },
  { section:"model · stage-2",
    before:"anthropic/claude-haiku-4-5",
    after:"anthropic/claude-haiku-4-5",
    kind:"unchanged", lineDelta:"—" },
];

const ParentDiff = () => (
  <div style={{padding:"22px 28px 0"}}>
    <SectionHeaderV title="What this experiment changed"
      sub="parent → experiment · 1 prompt section rewritten · 0 thresholds tuned · 0 agents added or removed"
      right={<>
        <Btn variant="ghost" dense icon="copy">Copy diff</Btn>
        <Btn variant="ghost" dense icon="ext">Full manifest</Btn>
      </>}/>

    <div style={{
      marginTop:14, border:"1px solid var(--border)", borderRadius:6,
      background:"var(--surface-card)",
    }}>
      {DIFF.map((d, i) => (
        <div key={d.section} style={{
          padding: d.kind === "changed" ? "0" : "10px 16px",
          borderTop: i === 0 ? "none" : "1px solid var(--border-soft)",
        }}>
          {d.kind === "changed" ? (
            <div>
              <div style={{
                padding:"10px 16px",
                borderBottom:"1px solid var(--border-soft)",
                display:"flex", justifyContent:"space-between", alignItems:"center",
                background:"rgba(0,230,118,0.04)",
              }}>
                <div style={{display:"flex", alignItems:"center", gap:8}}>
                  <span style={{
                    padding:"2px 6px", borderRadius:3,
                    border:"1px solid var(--gold-soft)", background:"var(--gold-bg)",
                  }}>
                    <span className="mono" style={{
                      fontSize:9.5, color:"var(--gold)", letterSpacing:"0.14em", fontWeight:600,
                    }}>CHANGED</span>
                  </span>
                  <span className="mono" style={{fontSize:12, color:"var(--text)", fontWeight:600}}>
                    {d.section}
                  </span>
                  <span className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>{d.lineDelta}</span>
                </div>
                <span className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>
                  fingerprint <span style={{color:"var(--text-2)"}}>c3e9…7a02</span>
                </span>
              </div>
              <div style={{
                display:"grid", gridTemplateColumns:"1fr 1fr", gap:0,
                borderTop:"1px solid var(--border-soft)",
              }}>
                <DiffSide tone="before" title="PARENT · btc-momentum-v3" code={d.before}/>
                <DiffSide tone="after"  title="VARIANT · v3.1.g"        code={d.after}/>
              </div>
            </div>
          ) : (
            <div style={{display:"flex", alignItems:"center", gap:10}}>
              <span style={{
                padding:"2px 6px", borderRadius:3,
                border:"1px solid var(--border-strong)", background:"transparent",
              }}>
                <span className="mono" style={{
                  fontSize:9.5, color:"var(--text-3)", letterSpacing:"0.14em",
                }}>UNCHANGED</span>
              </span>
              <span className="mono" style={{fontSize:12, color:"var(--text-2)"}}>{d.section}</span>
              <span className="mono" style={{marginLeft:"auto", fontSize:10.5, color:"var(--text-3)"}}>
                same as parent
              </span>
            </div>
          )}
        </div>
      ))}
    </div>
  </div>
);

const DiffSide = ({ tone, title, code }) => {
  const isAfter = tone === "after";
  const bg = isAfter ? "rgba(0,230,118,0.04)" : "rgba(255,77,77,0.03)";
  const bd = isAfter ? "rgba(0,230,118,0.15)" : "rgba(255,77,77,0.15)";
  const fg = isAfter ? "var(--gold)" : "var(--danger)";
  return (
    <div style={{
      borderLeft: isAfter ? `1px solid var(--border)` : "none",
      background:bg,
    }}>
      <div style={{
        padding:"7px 14px", borderBottom:`1px solid ${bd}`,
        display:"flex", alignItems:"center", gap:8,
      }}>
        <span className="mono" style={{
          fontSize:9.5, color:fg, letterSpacing:"0.14em", fontWeight:600,
        }}>{isAfter ? "+ AFTER" : "− BEFORE"}</span>
        <span className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>{title}</span>
      </div>
      <pre className="mono" style={{
        margin:0, padding:"12px 14px",
        fontSize:11.5, color: isAfter ? "var(--text)" : "var(--text-3)",
        whiteSpace:"pre-wrap", wordBreak:"break-word", lineHeight:1.55,
      }}>{code}</pre>
    </div>
  );
};

// ── Per-regime strip ──
const PerRegimeStrip = () => (
  <div style={{padding:"22px 28px 0"}}>
    <SectionHeaderV title="Per-regime evaluation"
      sub="anti-overfit gate requires positive Δ-Sharpe on ≥1 bull AND ≥1 bear · this experiment is kept on all 5"
      right={<Btn variant="ghost" dense icon="ext">Flight recorder</Btn>}/>
    <div style={{
      marginTop:14,
      display:"grid", gridTemplateColumns:"repeat(5, 1fr)", gap:10,
    }}>
      {REGIME_RESULTS.map((r) => {
        const meta = AR_REGIMES.find(rm => rm.id === r.regime);
        return <RegimeCard key={r.regime} r={r} kind={meta.kind}/>;
      })}
    </div>
  </div>
);

const RegimeCard = ({ r, kind }) => {
  const positive = r.delta >= 0;
  // Mini equity curve, seeded by regime
  let seed = 0;
  for (const c of r.regime) seed = (seed * 31 + c.charCodeAt(0)) >>> 0;
  const pts = [];
  let v = 0;
  for (let i = 0; i < 40; i++) {
    seed = (seed * 1103515245 + 12345) >>> 0;
    const noise = ((seed % 1000) / 1000 - 0.5) * 0.04;
    v += (r.delta * 0.06) + noise;
    pts.push(v);
  }
  const min = Math.min(...pts), max = Math.max(...pts);
  const w = 180, h = 36;
  const d = pts.map((p, i) => {
    const x = (i / (pts.length - 1)) * w;
    const y = h - ((p - min) / (max - min || 1)) * h;
    return `${i === 0 ? "M" : "L"} ${x.toFixed(1)} ${y.toFixed(1)}`;
  }).join(" ");
  return (
    <div style={{
      padding:"12px 12px", border:`1px solid ${positive ? "var(--gold-soft)" : "rgba(255,77,77,0.30)"}`,
      borderRadius:6, background:"var(--surface-card)",
    }}>
      <div style={{display:"flex", alignItems:"center", gap:6, marginBottom:8}}>
        <RegimeIcon kind={kind} size={11} color={REGIME_KIND_COLOR[kind]}/>
        <span className="mono" style={{fontSize:10.5, color:"var(--text-2)", fontWeight:600}}>{r.regime}</span>
      </div>

      <svg width="100%" viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" style={{display:"block", marginBottom:6}}>
        <path d={d} fill="none" stroke={positive ? "var(--gold)" : "var(--danger)"} strokeWidth="1.4"/>
      </svg>

      <div style={{display:"flex", justifyContent:"space-between", alignItems:"baseline"}}>
        <span className="mono" style={{
          fontSize:18, fontWeight:600, color: positive ? "var(--gold)" : "var(--danger)",
          letterSpacing:"-0.02em", lineHeight:1,
        }}>
          {positive ? "+" : ""}{r.delta.toFixed(2)} Δ
        </span>
        <span className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>S {r.sharpe.toFixed(2)}</span>
      </div>

      <div style={{
        marginTop:9, paddingTop:8, borderTop:"1px solid var(--border-soft)",
        display:"grid", gridTemplateColumns:"1fr 1fr", gap:6,
      }}>
        <MiniK k="ret"    v={r.ret}    tone={r.ret.startsWith("+") ? "gold" : "danger"}/>
        <MiniK k="dd"     v={r.dd}     tone="warn"/>
        <MiniK k="winrt"  v={r.wr}/>
        <MiniK k="trades" v={r.trades}/>
      </div>
    </div>
  );
};

const MiniK = ({ k, v, tone = "neutral" }) => {
  const col = tone === "gold" ? "var(--gold)" :
              tone === "warn" ? "var(--warn)" :
              tone === "danger" ? "var(--danger)" : "var(--text-2)";
  return (
    <div style={{display:"flex", justifyContent:"space-between", gap:4}}>
      <span className="mono" style={{fontSize:9.5, color:"var(--text-4)", letterSpacing:"0.08em"}}>{k}</span>
      <span className="mono" style={{fontSize:11, color:col, fontWeight:500}}>{v}</span>
    </div>
  );
};

// ── Flight recorder snippet ──
const TRACE_EVENTS = [
  { t:"04:08:42", stage:"intern", regime:"bull-q1-25", cycle_id:"01H8N7Z9", model:"haiku-4-5",
    msg:"Briefing emitted · bull_case: pri 0.78 · bear_case: pri 0.22 · regime_tag: trend-continuation",
    tone:"info" },
  { t:"04:08:43", stage:"trader", regime:"bull-q1-25", cycle_id:"01H8N7Z9", model:"haiku-4-5",
    msg:"TraderArm BUY · size_bps 75 · entry $67,420 · classifier=trend-continuation",
    tone:"gold" },
  { t:"04:08:44", stage:"risk-layer", regime:"bull-q1-25", cycle_id:"01H8N7Z9", model:"—",
    msg:"hard-cap OK · dyn-quota OK · isolated-margin OK → ALLOW",
    tone:"neutral" },
  { t:"04:09:11", stage:"intern", regime:"chop-q2-25", cycle_id:"01H8P3K4", model:"haiku-4-5",
    msg:"Briefing emitted · regime_tag: AMBIGUOUS · Stage 2 SKIP per experiment prompt change",
    tone:"warn" },
  { t:"04:09:12", stage:"trader", regime:"chop-q2-25", cycle_id:"01H8P3K4", model:"haiku-4-5",
    msg:"Stage-2 declined · empty TraderArm · reason=ambiguous-regime",
    tone:"neutral" },
  { t:"04:09:48", stage:"intern", regime:"bear-q3-24", cycle_id:"01H8Q1B7", model:"haiku-4-5",
    msg:"Briefing emitted · bull_case: pri 0.18 · bear_case: pri 0.82 · regime_tag: exhaustion",
    tone:"info" },
  { t:"04:09:49", stage:"trader", regime:"bear-q3-24", cycle_id:"01H8Q1B7", model:"haiku-4-5",
    msg:"TraderArm SELL · size_bps 40 · entry $58,210 · stop $58,910 · take-profit $56,820",
    tone:"danger" },
];

const FlightRecorder = () => (
  <div style={{padding:"22px 28px 0"}}>
    <SectionHeaderV title="Flight recorder · structured trace"
      sub="3 sampled cycles · full trace lives at runs/<run_id>/trace.ndjson — open the run for the rest"
      right={<>
        <Btn variant="ghost" dense icon="search">Filter</Btn>
        <Btn variant="ghost" dense icon="ext">Open run 01H8N7Z9</Btn>
      </>}/>
    <div style={{
      marginTop:14, border:"1px solid var(--border)", borderRadius:6,
      background:"var(--surface-card)", overflow:"hidden",
    }}>
      {/* legend */}
      <div style={{
        padding:"9px 16px", borderBottom:"1px solid var(--border)",
        display:"flex", gap:14, alignItems:"center",
      }}>
        <span className="ulabel" style={{fontSize:9, letterSpacing:"0.2em"}}>STAGES</span>
        <span style={{display:"flex", gap:10}}>
          <StageDot label="intern"     col="var(--info)"/>
          <StageDot label="trader"     col="var(--gold)"/>
          <StageDot label="risk-layer" col="var(--violet)"/>
          <StageDot label="execution"  col="var(--text-2)"/>
        </span>
        <span style={{marginLeft:"auto", display:"flex", gap:8}}>
          <Btn variant="chip" dense>Full</Btn>
          <Btn variant="ghost" dense>Errors only</Btn>
        </span>
      </div>

      {/* table */}
      <div style={{
        display:"grid",
        gridTemplateColumns:"80px 90px 130px 100px 100px 1fr",
        gap:10, alignItems:"center",
        padding:"8px 16px", borderBottom:"1px solid var(--border-soft)",
      }}>
        {["TIME","STAGE","REGIME","CYCLE_ID","MODEL","MESSAGE"].map((h, i) => (
          <div key={i} className="ulabel" style={{fontSize:9, letterSpacing:"0.2em"}}>{h}</div>
        ))}
      </div>
      {TRACE_EVENTS.map((e, i) => (
        <TraceRow key={i} e={e} last={i === TRACE_EVENTS.length - 1}/>
      ))}
    </div>
  </div>
);

const StageDot = ({ label, col }) => (
  <span style={{display:"inline-flex", alignItems:"center", gap:5}}>
    <span style={{width:6, height:6, borderRadius:"50%", background:col}}/>
    <span className="mono" style={{fontSize:10.5, color:"var(--text-2)"}}>{label}</span>
  </span>
);

const TraceRow = ({ e, last }) => {
  const stageCol = e.stage === "intern" ? "var(--info)" :
                   e.stage === "trader" ? "var(--gold)" :
                   e.stage === "risk-layer" ? "var(--violet)" : "var(--text-2)";
  const msgCol = e.tone === "gold" ? "var(--gold)" :
                 e.tone === "warn" ? "var(--warn)" :
                 e.tone === "danger" ? "var(--danger)" :
                 e.tone === "info" ? "var(--text)" : "var(--text-2)";
  return (
    <div style={{
      display:"grid",
      gridTemplateColumns:"80px 90px 130px 100px 100px 1fr",
      gap:10, alignItems:"center",
      padding:"7px 16px",
      borderBottom: last ? "none" : "1px solid var(--border-soft)",
    }}>
      <span className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>{e.t}</span>
      <span style={{display:"inline-flex", alignItems:"center", gap:5}}>
        <span style={{width:5, height:5, borderRadius:"50%", background:stageCol}}/>
        <span className="mono" style={{fontSize:11, color:stageCol, fontWeight:600}}>{e.stage}</span>
      </span>
      <span className="mono" style={{fontSize:10.5, color:"var(--text-2)"}}>{e.regime}</span>
      <span className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>{e.cycle_id}</span>
      <span className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>{e.model}</span>
      <span className="mono" style={{fontSize:11.5, color:msgCol, lineHeight:1.45}}>{e.msg}</span>
    </div>
  );
};

// ── Attestation detail ──
const ATT_DETAIL = [
  { name:"regime-verifier", token:"#0007", verdict:"ENDORSE", time:"04:11",
    note:"All 5 regimes pass regime-tag commitment. The experiment prompt explicitly classifies each bar — trace shows 142 trend-continuation tags, 88 exhaustion, 14 ambiguous-skip. Tags match the experiment's commitment fingerprint.",
    receipt:"c3e9a4f…" },
  { name:"diversity-check", token:"#0008", verdict:"ENDORSE", time:"04:09",
    note:"Variety score vs parent = 0.241 (threshold ≥ 0.18). Adds genuine novelty without collapsing toward an existing experiment in the lineage tree.",
    receipt:"7f2b1ad…" },
];

const AttestationDetail = () => (
  <div style={{padding:"22px 28px 0"}}>
    <SectionHeaderV title="Sign-off receipts"
      sub="2 of 2 local attesters approve · receipts can publish to chain via Marketplace"
      right={<Btn variant="ghost" dense icon="ext">Export receipts</Btn>}/>
    <div style={{
      marginTop:14, display:"grid", gridTemplateColumns:"1fr 1fr", gap:12,
    }}>
      {ATT_DETAIL.map((a) => (
        <div key={a.name} style={{
          border:"1px solid var(--gold-soft)", borderRadius:6,
          background:"linear-gradient(180deg, rgba(0,230,118,0.04), rgba(0,230,118,0.01))",
          padding:"14px 16px",
        }}>
          <div style={{display:"flex", alignItems:"center", gap:10, marginBottom:10}}>
            <div style={{
              width:30, height:30, borderRadius:5,
              background:"var(--gold-bg)", border:"1px solid var(--gold-soft)",
              display:"flex", alignItems:"center", justifyContent:"center",
            }}>
              <Icon name="shield" size={14} color="var(--gold)"/>
            </div>
            <div style={{flex:1}}>
              <div style={{display:"flex", alignItems:"center", gap:8}}>
                <span style={{fontSize:13.5, color:"var(--text)", fontWeight:600}}>{a.name}</span>
                <span className="mono" style={{fontSize:10.5, color:"var(--gold)"}}>local</span>
              </div>
              <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginTop:3}}>
                signed at {a.time}
              </div>
            </div>
            <GateBadge verdict={a.verdict === "ENDORSE" ? "PASS" : a.verdict === "QUESTION" ? "WARN" : "FAIL"} size="sm"/>
          </div>
          <p style={{
            margin:0, fontSize:12, color:"var(--text-2)", lineHeight:1.55,
          }}>{a.note}</p>
          <div style={{
            marginTop:10, paddingTop:10, borderTop:"1px solid var(--border-soft)",
            display:"flex", alignItems:"center", gap:8,
          }}>
            <span className="ulabel" style={{fontSize:9, letterSpacing:"0.16em"}}>fingerprint</span>
            <span className="mono" style={{fontSize:11, color:"var(--text-2)"}}>{a.receipt}</span>
            <span style={{marginLeft:"auto"}}>
              <span className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>signed locally</span>
            </span>
          </div>
        </div>
      ))}
    </div>
  </div>
);

// ── Decision strip — final operator review action ──
const DecisionStrip = () => (
  <div style={{padding:"22px 28px 28px"}}>
    <div style={{
      padding:"16px 18px", border:"1px solid var(--gold-soft)", borderRadius:6,
      background:"linear-gradient(180deg, rgba(0,230,118,0.05), rgba(0,230,118,0.01))",
      display:"grid", gridTemplateColumns:"1fr auto", gap:18, alignItems:"center",
    }}>
      <div>
        <div style={{display:"flex", alignItems:"center", gap:10}}>
          <Icon name="diamond" size={14} color="var(--gold)"/>
          <span style={{fontSize:14.5, color:"var(--text)", fontWeight:600}}>
            Ready to sign off at <span className="mono" style={{color:"var(--gold)"}}>06:14</span>
          </span>
          <span style={{color:"var(--text-4)"}}>·</span>
          <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>
            kept this cycle · queued for tonight's evening summary
          </span>
        </div>
        <div className="mono" style={{fontSize:11.5, color:"var(--text-2)", marginTop:8, lineHeight:1.55}}>
          On sign-off: this experiment lands in <span style={{color:"var(--text)"}}>btc-momentum</span> lineage
          as <span className="mono" style={{color:"var(--text)"}}>btc-momentum-v3.1.g</span>.
          You can <span style={{color:"var(--gold)"}}>activate for paper trading</span> before sign-off —
          14 days of live-paper data are required before the green Verified badge.
          Publishing to chain happens later from <span style={{color:"var(--gold)"}}>Marketplace</span> when you choose.
        </div>
      </div>
      <div style={{display:"flex", flexDirection:"column", gap:8, minWidth:200}}>
        <Btn variant="primary" icon="bolt" style={{justifyContent:"center"}}>Confirm sign-off</Btn>
        <Btn variant="ghost"   icon="info" style={{justifyContent:"center"}}>Hold for review</Btn>
        <Btn variant="danger"  dense       style={{justifyContent:"center"}}>Drop &amp; discard</Btn>
      </div>
    </div>
  </div>
);

window.ARVariant = ARVariant;
