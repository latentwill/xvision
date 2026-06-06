// Autoresearch · settings — /settings/autoresearch
// Cadence · experiment budget · models · regimes · gate thresholds · attesters.

const SectionHeaderS = window.ARSectionHeader;

const SettingsFrame = ({ children }) => (
  <div style={{
    background:"#000", width:"100%", minHeight:"100%",
    display:"grid", gridTemplateColumns:"200px 1fr", position:"relative",
  }}>{children}</div>
);

const ARSettings = () => (
  <SettingsFrame>
    <SideNav active="settings" marketplaceVisible={true} optimizerVisible={true}/>
    <main style={{display:"flex", flexDirection:"column", minWidth:0}}>
      <TopStatus breadcrumb={[
        { text:"SETTINGS" },
        { text:"optimizer", mono:true },
      ]}/>

      <div style={{flex:1, minHeight:0, display:"flex"}}>
        <window.SettingsSidebar active="optimizer"/>
        <div style={{flex:1, padding:"26px 32px 28px", display:"flex", flexDirection:"column", gap:18}}>

          {/* Page header */}
          <div style={{display:"flex", justifyContent:"space-between", alignItems:"flex-end"}}>
            <div>
              <h1 style={{margin:0, fontSize:28, fontWeight:600, letterSpacing:"-0.03em", lineHeight:1.1}}>
                Optimizer
              </h1>
              <div style={{marginTop:6, fontSize:13, color:"var(--text-2)"}}>
                Overnight loop that proposes &amp; evaluates strategy experiments · keeps survivors as lineage artifacts
              </div>
            </div>
            <div style={{display:"flex", gap:8}}>
              <Btn variant="ghost" icon="info">What is this?</Btn>
              <Btn variant="ghost" icon="ext">Open run history</Btn>
            </div>
          </div>

          {/* Master switch + schedule */}
          <MasterSwitchCard/>

          {/* Experiment budget */}
          <ExperimentBudgetCard/>

          {/* Models */}
          <ModelsCard/>

          {/* Regime set */}
          <RegimeSetCard/>

          {/* Gate thresholds */}
          <GateThresholdsCard/>

          {/* Attesters / signing */}
          <AttestersSettingsCard/>

          {/* Footer */}
          <div style={{
            padding:"14px 16px", border:"1px solid var(--border)", borderRadius:6,
            display:"flex", justifyContent:"space-between", alignItems:"center",
          }}>
            <div>
              <div style={{fontSize:12.5, color:"var(--text)"}}>Test the loop end-to-end before tonight's cycle?</div>
              <div className="mono" style={{fontSize:11, color:"var(--text-3)", marginTop:3}}>
                spawns 2 experiments · evals 2 regimes · skips sign-off · ~9 minutes
              </div>
            </div>
            <div style={{display:"flex", gap:8}}>
              <Btn variant="ghost">Reset to defaults</Btn>
              <Btn variant="primary" icon="bolt">Smoke test loop</Btn>
            </div>
          </div>

        </div>
      </div>
    </main>
  </SettingsFrame>
);

// ── Master switch + schedule ──
const MasterSwitchCard = () => (
  <Card
    title="Schedule"
    sub="When the loop runs"
    right={<ARStatusPill status="breeding"/>}
  >
    <div style={{padding:"18px 18px 16px", display:"grid", gridTemplateColumns:"1fr 1fr 1fr", gap:24}}>
      {/* Cadence */}
      <div>
        <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em", marginBottom:10}}>CADENCE</div>
        <div style={{display:"flex", flexDirection:"column", gap:7}}>
          {[
            { id:"nightly", label:"Every night",     desc:"23:00 local · default",  active:true },
            { id:"every-other", label:"Every other night", desc:"odd-day starts",   active:false },
            { id:"manual", label:"Manual only",      desc:"never auto-trigger",     active:false },
          ].map((c) => (
            <SettingRadio key={c.id} label={c.label} desc={c.desc} active={c.active}/>
          ))}
        </div>
      </div>

      {/* Window */}
      <div>
        <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em", marginBottom:10}}>WINDOW</div>
        <SettingField label="Start" value="23:00" mono/>
        <SettingField label="Hard cutoff" value="07:00" mono note="cycle is killed if still running"/>
        <SettingField label="Sign-off at" value="06:14" mono note="evening summary locked 30 min before cutoff"/>
      </div>

      {/* Concurrency */}
      <div>
        <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em", marginBottom:10}}>CONCURRENCY</div>
        <SettingSlider label="Lineages in parallel" min={1} max={8} value={3} ticks/>
        <SettingSlider label="Evals per lineage"     min={1} max={6} value={2} ticks/>
        <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginTop:8, lineHeight:1.5}}>
          ≈ <span style={{color:"var(--text-2)"}}>6 backtests</span> in flight at any time<br/>
          dyn-quota tokens · <span style={{color:"var(--warn)"}}>0.18 ETH/wk</span> est gas
        </div>
      </div>
    </div>
  </Card>
);

// ── Experiment budget ──
const ExperimentBudgetCard = () => {
  const experiments = [
    { id:"prompt-tweak",       weight:40, enabled:true  },
    { id:"threshold-tune",     weight:25, enabled:true  },
    { id:"agent-add",          weight:15, enabled:true  },
    { id:"agent-remove",       weight:5,  enabled:true  },
    { id:"regime-detect-swap", weight:10, enabled:true  },
    { id:"model-swap",         weight:5,  enabled:false },
  ];
  const total = experiments.filter(m => m.enabled).reduce((a, b) => a + b.weight, 0);
  return (
    <Card
      title="Experiment budget"
      sub="What kinds of experiments the optimizer proposes · how often"
      right={<>
        <Btn variant="ghost" dense icon="ext">Inspect proposer policy</Btn>
      </>}
    >
      <div style={{padding:"16px 18px 18px"}}>
        <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginBottom:12}}>
          experiments per cycle <span style={{color:"var(--text)"}}>14</span>
          <span style={{margin:"0 8px", color:"var(--text-4)"}}>·</span>
          weights <span style={{color:"var(--text)"}}>{total}</span> total
          <span style={{margin:"0 8px", color:"var(--text-4)"}}>·</span>
          1 change per experiment (single-edit policy)
        </div>
        <div style={{display:"grid", gridTemplateColumns:"repeat(2, 1fr)", gap:10}}>
          {experiments.map((m) => <ExperimentRow key={m.id} m={m} total={total}/>)}
        </div>
      </div>
    </Card>
  );
};

const ExperimentRow = ({ m, total }) => {
  const meta = AR_EXPERIMENTS[m.id];
  const pct = m.enabled ? (m.weight / total) * 100 : 0;
  return (
    <div style={{
      padding:"10px 12px",
      border:`1px solid ${m.enabled ? "var(--border)" : "var(--border-soft)"}`,
      borderRadius:5, opacity: m.enabled ? 1 : 0.55,
      display:"flex", flexDirection:"column", gap:8,
    }}>
      <div style={{display:"flex", alignItems:"center", gap:8}}>
        <ExperimentPill kind={m.id}/>
        <span style={{fontSize:11.5, color:"var(--text-2)"}}>{meta.desc}</span>
        <span style={{marginLeft:"auto", display:"flex", alignItems:"center", gap:6}}>
          <span className="mono" style={{fontSize:11, color:"var(--text)", fontWeight:600}}>
            {m.weight}
          </span>
          <Toggle on={m.enabled}/>
        </span>
      </div>
      <div style={{
        height:3, borderRadius:2, background:"var(--surface-elev)", overflow:"hidden",
      }}>
        <div style={{
          width:`${pct}%`, height:"100%",
          background: m.enabled ? "var(--gold)" : "var(--text-4)",
        }}/>
      </div>
    </div>
  );
};

// ── Models ──
const ModelsCard = () => (
  <Card
    title="Models"
    sub="Which providers/models the optimizer uses to propose, eval, and judge"
  >
    <div style={{padding:"16px 18px", display:"grid", gridTemplateColumns:"1fr 1fr 1fr", gap:20}}>
      <ModelSlot stage="PROPOSER" desc="proposes the next experiment"
        model="Claude · Sonnet 4.5" provider="Anthropic" tokens="≈ 200k/cycle" cost="~$1.40"/>
      <ModelSlot stage="EVALUATOR" desc="runs Stage-1 / Stage-2 in backtest"
        model="Claude · Haiku 4.5" provider="Anthropic" tokens="≈ 8.2M/cycle" cost="~$3.10"/>
      <ModelSlot stage="JUDGE" desc="scores Δ-Sharpe vs parent · signs off inline"
        model="GPT-5" provider="OpenAI" tokens="≈ 80k/cycle" cost="~$0.40"/>
    </div>
  </Card>
);

const ModelSlot = ({ stage, desc, model, provider, tokens, cost }) => (
  <div style={{
    padding:"12px 14px", border:"1px solid var(--border)", borderRadius:5,
    background:"var(--surface-card)",
  }}>
    <div className="ulabel" style={{fontSize:9, letterSpacing:"0.22em"}}>{stage}</div>
    <div style={{fontSize:11.5, color:"var(--text-3)", marginTop:4}}>{desc}</div>
    <div style={{
      marginTop:10, padding:"7px 10px", border:"1px solid var(--border-strong)", borderRadius:4,
      background:"var(--surface-elev)",
      display:"flex", justifyContent:"space-between", alignItems:"center",
    }}>
      <div>
        <div className="mono" style={{fontSize:12, color:"var(--text)", fontWeight:600}}>{model}</div>
        <div className="mono" style={{fontSize:10, color:"var(--text-3)", marginTop:2}}>{provider}</div>
      </div>
      <Icon name="chevD" size={10} color="var(--text-3)" sw={2}/>
    </div>
    <div style={{
      marginTop:8, display:"flex", justifyContent:"space-between",
    }}>
      <span className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>{tokens}</span>
      <span className="mono" style={{fontSize:10.5, color:"var(--gold)"}}>{cost}</span>
    </div>
  </div>
);

// ── Regime set ──
const REGIME_SET = [
  { id:"bull-q1-25",       label:"bull-q1-25",       kind:"bull",  enabled:true,  required:false },
  { id:"chop-q2-25",       label:"chop-q2-25",       kind:"chop",  enabled:true,  required:false },
  { id:"bear-q3-24",       label:"bear-q3-24",       kind:"bear",  enabled:true,  required:true  },
  { id:"flash-crash-24-08",label:"flash-crash-24-08",kind:"shock", enabled:true,  required:true  },
  { id:"chop-q4-23",       label:"chop-q4-23",       kind:"chop",  enabled:true,  required:false },
  { id:"bull-q4-21",       label:"bull-q4-21",       kind:"bull",  enabled:false, required:false },
  { id:"bear-q2-22",       label:"bear-q2-22",       kind:"bear",  enabled:false, required:false },
];

const RegimeSetCard = () => (
  <Card
    title="Regime set"
    sub="Historical windows experiments are evaluated against · gate locks in pre-committed weights"
    right={<Btn variant="ghost" dense icon="plus">Add custom regime</Btn>}
  >
    <div style={{padding:"14px 0 4px"}}>
      <div style={{
        display:"grid",
        gridTemplateColumns:"32px 22px 1fr 90px 90px 90px 80px 80px",
        gap:10, alignItems:"center",
        padding:"8px 18px", borderBottom:"1px solid var(--border-soft)",
      }}>
        {["","","Regime","Kind","Window","Bars","Required","On"].map((h, i) => (
          <div key={i} className="ulabel" style={{fontSize:9, letterSpacing:"0.2em"}}>{h}</div>
        ))}
      </div>
      {REGIME_SET.map((r, i) => (
        <div key={r.id} style={{
          display:"grid",
          gridTemplateColumns:"32px 22px 1fr 90px 90px 90px 80px 80px",
          gap:10, alignItems:"center",
          padding:"10px 18px",
          borderBottom: i < REGIME_SET.length-1 ? "1px solid var(--border-soft)" : "none",
          opacity: r.enabled ? 1 : 0.55,
        }}>
          <span style={{
            display:"inline-flex", alignItems:"center", justifyContent:"center",
            width:24, height:24, borderRadius:4,
            background: r.enabled ? `color-mix(in oklab, ${REGIME_KIND_COLOR[r.kind]} 12%, transparent)` : "var(--surface-elev)",
            border:`1px solid ${r.enabled ? REGIME_KIND_COLOR[r.kind] : "var(--border-strong)"}`,
          }}>
            <RegimeIcon kind={r.kind} size={11} color={REGIME_KIND_COLOR[r.kind]}/>
          </span>
          <span style={{
            display:"inline-flex", alignItems:"center", justifyContent:"center",
            width:18, height:18, borderRadius:3,
            border:"1px solid var(--text-3)", cursor:"grab",
            color:"var(--text-3)", fontSize:8,
          }}>≡</span>
          <span className="mono" style={{fontSize:12, color:"var(--text)", fontWeight:600}}>{r.label}</span>
          <span className="mono" style={{fontSize:11, color:"var(--text-2)"}}>{r.kind}</span>
          <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>
            {AR_REGIMES.find(rm => rm.id === r.id)?.window || "—"}
          </span>
          <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>
            {AR_REGIMES.find(rm => rm.id === r.id)?.bars || "—"}
          </span>
          {r.required ? (
            <span style={{
              padding:"2px 7px", borderRadius:3,
              border:"1px solid var(--gold-soft)", background:"var(--gold-bg)",
              width:"fit-content",
            }}>
              <span className="mono" style={{
                fontSize:9.5, color:"var(--gold)", letterSpacing:"0.14em", fontWeight:600,
              }}>REQUIRED</span>
            </span>
          ) : (
            <span className="mono" style={{fontSize:10.5, color:"var(--text-4)"}}>—</span>
          )}
          <Toggle on={r.enabled}/>
        </div>
      ))}
    </div>
    <div style={{
      padding:"10px 18px", borderTop:"1px solid var(--border)", background:"rgba(255,176,32,0.04)",
      display:"flex", alignItems:"center", gap:8,
    }}>
      <Icon name="info" size={12} color="var(--warn)"/>
      <span className="mono" style={{fontSize:11, color:"var(--text-2)"}}>
        Required regimes are part of the anti-overfit gate · disable cautiously
      </span>
    </div>
  </Card>
);

// ── Gate thresholds ──
const GateThresholdsCard = () => (
  <Card
    title="Anti-overfit gate"
    sub="Minimum bar an experiment must clear to be kept"
  >
    <div style={{padding:"16px 18px", display:"grid", gridTemplateColumns:"1fr 1fr", gap:20}}>
      <div>
        <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em", marginBottom:10}}>HARD RULES</div>
        <div style={{display:"flex", flexDirection:"column", gap:8}}>
          <HardRule label="Positive Δ-Sharpe in ≥1 bull regime"  on/>
          <HardRule label="Positive Δ-Sharpe in ≥1 bear OR shock regime" on/>
          <HardRule label="Variety score ≥ 0.18 from parent"  on/>
          <HardRule label="Min trades / regime"  on rightLabel="≥ 30"/>
          <HardRule label="Experiment fingerprint recorded" on/>
        </div>
      </div>
      <div>
        <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em", marginBottom:10}}>NUMERIC THRESHOLDS</div>
        <SettingSlider label="Min Δ-Sharpe to keep"      min={0.0} max={0.5}  value={0.05} step={0.01} value2="+0.05 Δ"/>
        <SettingSlider label="Max drawdown · any regime" min={-0.30} max={0.0} value={-0.12} step={0.01} value2="-12.0%"/>
        <SettingSlider label="Min variety score"          min={0.0} max={0.5}  value={0.18} step={0.01} value2="0.18"/>

        <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginTop:12, lineHeight:1.5}}>
          tightening these reduces the number of experiments kept but raises post-sign-off trust
        </div>
      </div>
    </div>
  </Card>
);

const HardRule = ({ label, on, rightLabel }) => (
  <label style={{
    display:"flex", alignItems:"center", gap:10,
    padding:"7px 10px", border:"1px solid var(--border-soft)", borderRadius:4,
  }}>
    <span style={{
      width:13, height:13, borderRadius:2,
      border:`1px solid ${on ? "var(--gold)" : "var(--border-strong)"}`,
      background: on ? "var(--gold)" : "transparent",
      display:"flex", alignItems:"center", justifyContent:"center", flexShrink:0,
    }}>
      {on && (
        <svg width="9" height="9" viewBox="0 0 9 9" fill="none"
          stroke="#001A0A" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M1.5 4.5L4 7l4-5"/>
        </svg>
      )}
    </span>
    <span style={{fontSize:12.5, color:"var(--text-2)", flex:1}}>{label}</span>
    {rightLabel && (
      <span className="mono" style={{fontSize:11, color:"var(--text)", fontWeight:600}}>{rightLabel}</span>
    )}
  </label>
);

// ── Attesters / signing keys ──
const AttestersSettingsCard = () => (
  <Card
    title="Attesters &amp; signing"
    sub="Local co-signers · operator key separation"
    right={<Btn variant="ghost" dense icon="plus">Add attester</Btn>}
  >
    <div style={{padding:"14px 18px", display:"grid", gridTemplateColumns:"1fr 1fr", gap:14}}>
      <div>
        <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em", marginBottom:10}}>ATTESTER AGENTS</div>
        {[
          { name:"regime-verifier", token:"#0007", note:"signs sign-off receipts for every kept experiment", active:true },
          { name:"diversity-check", token:"#0008", note:"signs when variety score crosses threshold",        active:true },
        ].map((a) => (
          <div key={a.name} style={{
            padding:"10px 12px", border:"1px solid var(--border)", borderRadius:5,
            marginBottom:8,
            display:"flex", alignItems:"center", gap:10,
          }}>
            <div style={{
              width:28, height:28, borderRadius:4,
              background:"var(--gold-bg)", border:"1px solid var(--gold-soft)",
              display:"flex", alignItems:"center", justifyContent:"center", flexShrink:0,
            }}>
              <Icon name="shield" size={13} color="var(--gold)"/>
            </div>
            <div style={{flex:1, minWidth:0}}>
              <div style={{display:"flex", alignItems:"center", gap:8}}>
                <span style={{fontSize:12.5, color:"var(--text)", fontWeight:600}}>{a.name}</span>
                <span className="mono" style={{fontSize:10.5, color:"var(--gold)"}}>local</span>
                <ARStatusPill status="breeding"/>
              </div>
              <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginTop:3}}>{a.note}</div>
            </div>
            <Toggle on={a.active}/>
          </div>
        ))}
      </div>
      <div>
        <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em", marginBottom:10}}>OPERATOR KEY SEPARATION</div>
        <div style={{padding:"10px 12px", border:"1px solid var(--border)", borderRadius:5}}>
          <div style={{display:"flex", justifyContent:"space-between", alignItems:"center", padding:"4px 0"}}>
            <span style={{fontSize:12, color:"var(--text-2)"}}>Your signing key</span>
            <span className="mono" style={{fontSize:11, color:"var(--text)"}}>7f2b…91c4</span>
          </div>
          <div style={{display:"flex", justifyContent:"space-between", alignItems:"center", padding:"4px 0"}}>
            <span style={{fontSize:12, color:"var(--text-2)"}}>On-chain wallet (optional)</span>
            <span className="mono" style={{fontSize:11, color:"var(--gold)"}}>0xa83e…f12d4</span>
          </div>
          <div style={{display:"flex", justifyContent:"space-between", alignItems:"center", padding:"4px 0"}}>
            <span style={{fontSize:12, color:"var(--text-2)"}}>Per-cycle signing key</span>
            <span className="mono" style={{fontSize:11, color:"var(--text)"}}>rotates per cycle</span>
          </div>
          <div className="mono" style={{
            fontSize:10, color:"var(--text-3)", marginTop:10, lineHeight:1.55, paddingTop:8,
            borderTop:"1px solid var(--border-soft)",
          }}>
            keys are deliberately distinct · sign-offs are local · the on-chain wallet only matters if you publish to Marketplace
          </div>
        </div>
      </div>
    </div>
  </Card>
);

// ── Reusable settings widgets ──

const SettingRadio = ({ label, desc, active }) => (
  <label style={{
    display:"flex", alignItems:"flex-start", gap:10,
    padding:"9px 11px",
    border:`1px solid ${active ? "var(--gold-soft)" : "var(--border-soft)"}`,
    background: active ? "var(--gold-bg)" : "transparent",
    borderRadius:4, cursor:"pointer",
  }}>
    <span style={{
      width:13, height:13, borderRadius:"50%",
      border:`1.5px solid ${active ? "var(--gold)" : "var(--text-3)"}`,
      marginTop:2, flexShrink:0,
      background: active ? "radial-gradient(circle, var(--gold) 0 40%, transparent 41%)" : "transparent",
    }}/>
    <div>
      <div style={{fontSize:12.5, fontWeight:500, color: active ? "var(--gold)" : "var(--text)"}}>{label}</div>
      <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginTop:2}}>{desc}</div>
    </div>
  </label>
);

const SettingField = ({ label, value, mono, note }) => (
  <div style={{marginBottom:10}}>
    <div style={{display:"flex", alignItems:"baseline", justifyContent:"space-between", marginBottom:5}}>
      <span style={{fontSize:11.5, color:"var(--text-2)"}}>{label}</span>
    </div>
    <div style={{
      padding:"5px 10px", border:"1px solid var(--border-strong)", borderRadius:3,
      background:"var(--surface-elev)",
    }}>
      <span className={mono ? "mono" : ""} style={{fontSize:13, color:"var(--text)"}}>{value}</span>
    </div>
    {note && <div className="mono" style={{fontSize:9.5, color:"var(--text-4)", marginTop:3}}>{note}</div>}
  </div>
);

const SettingSlider = ({ label, min, max, value, step = 1, ticks = false, value2 }) => {
  const pct = ((value - min) / (max - min)) * 100;
  return (
    <div style={{marginBottom:14}}>
      <div style={{display:"flex", alignItems:"baseline", justifyContent:"space-between", marginBottom:5}}>
        <span style={{fontSize:11.5, color:"var(--text-2)"}}>{label}</span>
        <span className="mono" style={{fontSize:12, color:"var(--text)", fontWeight:600}}>{value2 || value}</span>
      </div>
      <div style={{position:"relative", height:24, padding:"10px 0"}}>
        <div style={{
          position:"absolute", left:0, right:0, top:11, height:3, borderRadius:2,
          background:"var(--border-strong)",
        }}/>
        <div style={{
          position:"absolute", left:0, width:`${pct}%`, top:11, height:3, borderRadius:2,
          background:"var(--gold)",
        }}/>
        {ticks && Array.from({length: max - min + 1}).map((_, i) => (
          <div key={i} style={{
            position:"absolute", left:`${(i / (max - min)) * 100}%`, top:9,
            width:1, height:7, background:"var(--text-4)", transform:"translateX(-0.5px)",
          }}/>
        ))}
        <div style={{
          position:"absolute", left:`${pct}%`, top:6,
          width:12, height:12, borderRadius:"50%",
          background:"#000", border:"2px solid var(--gold)",
          transform:"translateX(-6px)", cursor:"pointer",
        }}/>
      </div>
      <div style={{
        display:"flex", justifyContent:"space-between", marginTop:-2,
        fontFamily:"'Geist Mono', monospace", fontSize:9.5, color:"var(--text-4)",
      }}>
        <span>{min}</span>
        <span>{max}</span>
      </div>
    </div>
  );
};

const Toggle = ({ on }) => (
  <span style={{
    width:30, height:17, borderRadius:9, position:"relative", flexShrink:0,
    background: on ? "var(--gold)" : "var(--border-strong)",
    transition:"background 0.18s", cursor:"pointer",
  }}>
    <span style={{
      position:"absolute", top:2, left: on ? 15 : 2,
      width:13, height:13, borderRadius:"50%", background:"#000",
      transition:"left 0.18s",
    }}/>
  </span>
);

window.ARSettings = ARSettings;
