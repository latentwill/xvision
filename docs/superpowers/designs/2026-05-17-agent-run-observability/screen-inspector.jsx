// xvn — Inspector / Authoring
const InspectorScreen = () => {
  return (
    <div className="shell" style={{gridTemplateColumns: "200px 220px 1fr 280px"}}>
      <Sidebar active="strategies" />
      {/* Bundle outline */}
      <aside style={{background: "var(--surface-sidebar)", borderRight: "1px solid var(--border-soft)", padding: "24px 18px", overflow: "hidden"}}>
        <div style={{fontSize: 11, color: "var(--text-3)", textTransform: "uppercase", letterSpacing: "0.08em", marginBottom: 10}}>Manifest</div>
        <div style={{display: "flex", flexDirection: "column", gap: 4, marginBottom: 18}}>
          <div style={{padding: "5px 10px", fontSize: 13, color: "var(--text-2)"}}>Identity</div>
          <div style={{padding: "5px 10px", fontSize: 13, color: "var(--text-2)"}}>Eval attestations</div>
        </div>
        <div style={{fontSize: 11, color: "var(--text-3)", textTransform: "uppercase", letterSpacing: "0.08em", marginBottom: 10}}>Layers</div>
        <div style={{display: "flex", flexDirection: "column", gap: 2, marginBottom: 18}}>
          {[
            ["①", "Data", false],
            ["②", "Regime classifier", false, "LLM"],
            ["③", "Intern", true, "LLM"],
            ["④", "Trader", false, "LLM"],
            ["⑤", "Entry / Exit rules", false],
            ["⑥", "Risk", false],
            ["⑦", "Execution", false],
          ].map(([n, l, active, tag], i) => (
            <div key={i} style={{
              padding: "6px 10px", fontSize: 13,
              color: active ? "var(--text)" : "var(--text-2)",
              background: active ? "rgba(212,165,71,0.08)" : "transparent",
              borderLeft: active ? "2px solid var(--gold)" : "2px solid transparent",
              display: "flex", justifyContent: "space-between", alignItems: "center",
              borderRadius: 2,
            }}>
              <span><span className="mute" style={{marginRight: 8}}>{n}</span>{l}</span>
              {tag && <span style={{fontSize: 9, color: "var(--gold)", border: "1px solid rgba(212,165,71,0.3)", padding: "1px 5px", borderRadius: 2}}>{tag}</span>}
            </div>
          ))}
        </div>
        <div style={{fontSize: 11, color: "var(--text-3)", textTransform: "uppercase", letterSpacing: "0.08em", marginBottom: 10}}>Validation</div>
        <div style={{padding: "6px 10px", fontSize: 13, color: "var(--warn)"}}><span className="dot warn"/>2 warnings, 0 errors</div>
        <div style={{position: "absolute", bottom: 24, left: 218, fontSize: 11, color: "var(--text-3)", fontFamily: "JetBrains Mono, monospace"}}>Bundle: 0xa83…f12</div>
      </aside>

      {/* Center — split editor */}
      <main className="main" style={{padding: "28px 28px 0", overflow: "hidden"}}>
        <div style={{display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 8}}>
          <div>
            <div style={{fontSize: 11, color: "var(--text-3)", letterSpacing: "0.08em", textTransform: "uppercase", marginBottom: 4}}>Authoring · eth-mr-v3</div>
            <h1 className="serif" style={{margin: 0, fontSize: 30}}>Intern <span className="mute" style={{fontSize: 16, fontFamily: "Inter"}}>· LLM slot</span></h1>
          </div>
          <div style={{display: "flex", gap: 8}}>
            <button className="btn ghost">Test slot</button>
            <button className="btn ghost">Save draft</button>
            <button className="btn primary">Run eval</button>
          </div>
        </div>

        <div style={{display: "grid", gridTemplateColumns: "1fr 1fr", gap: 18, marginTop: 18, height: "calc(100% - 80px)"}}>
          {/* Left pane — form */}
          <div className="card" style={{padding: "18px 20px", overflow: "hidden"}}>
            <div style={{display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 14}}>
              <span className="serif" style={{fontSize: 18}}>Slot configuration</span>
              <span style={{fontSize: 11, color: "var(--text-2)"}}><span className="dot gold"/>Use this agent</span>
            </div>

            <div style={{display: "grid", gap: 12, fontSize: 12}}>
              <div>
                <label style={{color: "var(--text-2)", fontSize: 11, textTransform: "uppercase", letterSpacing: "0.06em"}}>Model</label>
                <div style={{padding: "8px 10px", border: "1px solid var(--border)", borderRadius: 4, marginTop: 4, background: "var(--surface-elev)", fontFamily: "JetBrains Mono, monospace", display: "flex", justifyContent: "space-between"}}>
                  <span>anthropic/claude-haiku-4-5</span><span className="mute">▾</span>
                </div>
              </div>
              <div>
                <label style={{color: "var(--text-2)", fontSize: 11, textTransform: "uppercase", letterSpacing: "0.06em"}}>System prompt</label>
                <div style={{padding: "12px", border: "1px solid var(--border)", borderRadius: 4, marginTop: 4, background: "var(--surface-elev)", fontFamily: "JetBrains Mono, monospace", fontSize: 11.5, lineHeight: 1.6, color: "var(--text)", minHeight: 220}}>
                  <div className="mute"># Intern — pre-trade analyst</div>
                  <div>You are an intern reviewing a candidate trade.</div>
                  <div>Given OHLCV + indicators + regime, return JSON:</div>
                  <div>{"  "}<span style={{color: "var(--gold)"}}>action</span>: <span className="up">"long" | "short" | "skip"</span></div>
                  <div>{"  "}<span style={{color: "var(--gold)"}}>conviction</span>: <span className="up">0.0–1.0</span></div>
                  <div>{"  "}<span style={{color: "var(--gold)"}}>reasoning</span>: <span className="up">&lt;= 2 sentences</span></div>
                  <div style={{marginTop: 8}}>Bias: mean-reverts on RSI &lt; 30 with bb_lower touch.</div>
                  <div>Skip when: ATR-multiple &gt; 3 or regime = chop.</div>
                </div>
              </div>
              <div style={{display: "flex", gap: 12}}>
                <div style={{flex: 1}}>
                  <label style={{color: "var(--text-2)", fontSize: 11, textTransform: "uppercase", letterSpacing: "0.06em"}}>Tools allowed</label>
                  <div style={{display: "flex", gap: 6, marginTop: 6, flexWrap: "wrap"}}>
                    <span className="pill">indicator_panel</span>
                    <span className="pill">regime_lookup</span>
                    <span className="pill">+ Add</span>
                  </div>
                </div>
                <div style={{width: 110}}>
                  <label style={{color: "var(--text-2)", fontSize: 11, textTransform: "uppercase", letterSpacing: "0.06em"}}>Max tokens</label>
                  <div style={{padding: "6px 10px", border: "1px solid var(--border)", borderRadius: 4, marginTop: 4, background: "var(--surface-elev)", fontFamily: "JetBrains Mono, monospace"}}>1,200</div>
                </div>
              </div>
            </div>
          </div>

          {/* Right pane — live preview */}
          <div className="card" style={{padding: "18px 20px", display: "flex", flexDirection: "column", overflow: "hidden"}}>
            <div style={{display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 14}}>
              <span className="serif" style={{fontSize: 18}}>Preview decision</span>
              <span className="pill gold">Fixture: BTC/USD · 2025-01-15 08:00 ▾</span>
            </div>
            <div style={{display: "flex", gap: 10, fontSize: 12, color: "var(--text-2)", marginBottom: 14, alignItems: "center"}}>
              <span><span className="dot gold"/>Auto-rerun on (2s debounce)</span>
              <span style={{marginLeft: "auto"}} className="mute mono">Last preview: 1,420 tokens · ~$0.012</span>
            </div>

            <div style={{padding: 12, border: "1px solid var(--border)", borderRadius: 4, background: "var(--surface-elev)", fontFamily: "JetBrains Mono, monospace", fontSize: 11, marginBottom: 12}}>
              <div className="mute" style={{marginBottom: 6}}>Inputs ▾</div>
              <div>{`{ ohlcv_history: 60 candles, indicator_panel: { rsi: 27.4, bb_pos: -0.92, atr_mult: 1.8 }, regime: "bull-pullback" }`}</div>
            </div>

            <div style={{flex: 1, padding: 14, border: "1px solid rgba(212,165,71,0.25)", borderRadius: 4, background: "rgba(212,165,71,0.04)", fontFamily: "JetBrains Mono, monospace", fontSize: 12, lineHeight: 1.7}}>
              <div style={{color: "var(--gold)", marginBottom: 8}}><span className="dot gold"/>Streaming complete · 47 tokens</div>
              <div>{"{"}</div>
              <div>{"  "}<span style={{color: "var(--gold)"}}>"action"</span>: <span className="up">"long"</span>,</div>
              <div>{"  "}<span style={{color: "var(--gold)"}}>"conviction"</span>: <span className="up">0.72</span>,</div>
              <div>{"  "}<span style={{color: "var(--gold)"}}>"reasoning"</span>: <span style={{color: "var(--text)"}}>"RSI deeply oversold (27.4) with bb_lower touch; ATR within bounds; bull regime favors mean-revert long."</span></div>
              <div>{"}"}</div>
              <div className="mute" style={{marginTop: 12, fontSize: 11}}>Δ vs previous: action unchanged · conviction +0.04</div>
            </div>
          </div>
        </div>
      </main>

      {/* Right rail — validation + token estimate */}
      <aside className="rail" style={{padding: "28px 20px"}}>
        <div>
          <h3>Validation</h3>
          <div style={{display: "flex", flexDirection: "column", gap: 10, fontSize: 12}}>
            <div style={{display: "flex", gap: 8, alignItems: "flex-start"}}>
              <span className="dot warn" style={{marginTop: 6}}/>
              <div>
                <div style={{color: "var(--text)"}}>Regime classifier missing fixture</div>
                <div className="mute" style={{fontSize: 11}}>Add a chop fixture to validate.</div>
              </div>
            </div>
            <div style={{display: "flex", gap: 8, alignItems: "flex-start"}}>
              <span className="dot warn" style={{marginTop: 6}}/>
              <div>
                <div style={{color: "var(--text)"}}>Token budget exceeds soft limit</div>
                <div className="mute" style={{fontSize: 11}}>53.5k vs 50k target.</div>
              </div>
            </div>
          </div>
        </div>

        <div>
          <h3>Estimated tokens / run</h3>
          <div style={{display: "flex", flexDirection: "column", gap: 6, fontFamily: "JetBrains Mono, monospace", fontSize: 12}}>
            <div style={{display: "flex", justifyContent: "space-between"}}><span className="mute">input</span><span>45,000</span></div>
            <div style={{display: "flex", justifyContent: "space-between"}}><span className="mute">output</span><span>8,500</span></div>
            <hr className="hr"/>
            <div style={{display: "flex", justifyContent: "space-between"}}><span>total</span><span className="up">53,500</span></div>
          </div>
        </div>

        <div>
          <h3>Bundle JSON</h3>
          <div style={{padding: 10, border: "1px solid var(--border)", background: "var(--surface-elev)", borderRadius: 4, fontFamily: "JetBrains Mono, monospace", fontSize: 10.5, color: "var(--text-2)", lineHeight: 1.5}}>
            {`{
  "name": "eth-mr-v3",
  "template": "mean_reversion",
  "regime": { … },
  "intern": { … }
  ...`}
          </div>
        </div>
      </aside>
    </div>
  );
};
window.InspectorScreen = InspectorScreen;
