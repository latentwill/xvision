// xvn — Setup wizard
const SetupScreen = () => {
  return (
    <div className="shell">
      <Sidebar active="strategies" />
      <main className="main" style={{padding: 0, display: "grid", gridTemplateColumns: "1fr 1fr", gap: 0}}>
        {/* Chat column */}
        <section style={{padding: "36px 36px 0", borderRight: "1px solid var(--border-soft)", display: "flex", flexDirection: "column", height: 900, overflow: "hidden"}}>
          <div style={{display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 24}}>
            <div>
              <div className="serif-i" style={{fontSize: 30, lineHeight: 1.2}}>Welcome to xvn.</div>
              <div className="mute" style={{fontSize: 13, marginTop: 4}}>Setup agent · <span className="dot gold" style={{marginLeft: 4}}/>Online</div>
            </div>
          </div>

          <div style={{flex: 1, overflow: "hidden", display: "flex", flexDirection: "column", gap: 16, paddingRight: 8}}>
            {/* Agent message */}
            <div style={{borderLeft: "2px solid var(--gold)", paddingLeft: 14, fontSize: 13.5, lineHeight: 1.6, color: "var(--text)"}}>
              Hi! I'm the xvn setup agent. I'll help you build or pick an AI trading bot. What's your goal today?
            </div>

            {/* User message */}
            <div style={{alignSelf: "flex-end", maxWidth: "78%", background: "var(--surface-elev)", padding: "10px 14px", borderRadius: 4, fontSize: 13.5, lineHeight: 1.55, border: "1px solid var(--border)"}}>
              I want to mean-revert ETH on the 15-minute timeframe. Skip chop.
            </div>

            <div style={{borderLeft: "2px solid var(--gold)", paddingLeft: 14, fontSize: 13.5, lineHeight: 1.6, color: "var(--text)"}}>
              Good shape. I'll start from the <span className="mono" style={{color: "var(--gold)"}}>mean_reversion</span> template and wire a chop-aware regime classifier.
              <div style={{marginTop: 10, padding: 10, background: "var(--surface-elev)", border: "1px solid var(--border)", borderRadius: 4, fontFamily: "JetBrains Mono, monospace", fontSize: 11.5}}>
                <div className="mute"># Tool: list_templates</div>
                <div style={{color: "var(--gold)", marginTop: 4}}>Found 7 templates. Selected: mean_reversion ✓</div>
              </div>
              <div style={{marginTop: 12}}>I'm drafting <span className="mono" style={{color: "var(--gold)"}}>eth-mr-v3</span>. Want to keep RSI &lt; 30 as the entry trigger, or use Bollinger band touches?</div>
            </div>

            {/* User message */}
            <div style={{alignSelf: "flex-end", maxWidth: "78%", background: "var(--surface-elev)", padding: "10px 14px", borderRadius: 4, fontSize: 13.5, lineHeight: 1.55, border: "1px solid var(--border)"}}>
              Both — and let the LLM decide which is stronger.
            </div>

            {/* Agent typing */}
            <div style={{borderLeft: "2px solid var(--gold)", paddingLeft: 14, fontSize: 13.5, lineHeight: 1.6, color: "var(--text-2)"}}>
              <span style={{color: "var(--gold)"}}>●</span> Drafting Intern slot prompt…
            </div>
          </div>

          {/* Quick replies */}
          <div style={{display: "flex", gap: 8, padding: "16px 0", flexWrap: "wrap"}}>
            <span className="pill gold">Try a free strategy</span>
            <span className="pill gold">Build from a template</span>
            <span className="pill gold">Diagnose a recent run</span>
          </div>

          {/* Composer */}
          <div style={{padding: "12px 14px", border: "1px solid var(--border)", borderRadius: 4, background: "var(--surface-elev)", marginBottom: 24, display: "flex", alignItems: "flex-end", gap: 12}}>
            <div style={{flex: 1, color: "var(--text-2)", fontSize: 13, minHeight: 40, paddingTop: 4}}>Tell me what you want to build…</div>
            <button className="btn ghost" style={{padding: "6px 10px", fontSize: 12}}>+ Attach</button>
            <button className="btn primary" style={{padding: "6px 14px"}}>Send <span style={{opacity: 0.5}}>⌘↵</span></button>
          </div>
        </section>

        {/* Live progress */}
        <section style={{padding: "36px 36px 0", height: 900, overflow: "hidden", display: "flex", flexDirection: "column"}}>
          <div style={{display: "flex", justifyContent: "space-between", alignItems: "flex-end", marginBottom: 24}}>
            <div>
              <div style={{fontSize: 11, color: "var(--text-3)", letterSpacing: "0.08em", textTransform: "uppercase", marginBottom: 4}}>Strategy in progress</div>
              <div className="serif" style={{fontSize: 28}}>eth-mr-v3</div>
            </div>
            <span className="pill gold"><span className="dot gold"/>Drafting</span>
          </div>

          <div style={{display: "flex", flexDirection: "column", gap: 16, flex: 1, overflow: "hidden"}}>
            {[
              ["Template", [["Selected", "mean_reversion", true]]],
              ["Agents", [
                ["Regime", "claude-haiku-4-5", "ready"],
                ["Intern", "claude-haiku-4-5", "drafting"],
                ["Trader", "claude-sonnet-4-5", "ready"],
              ]],
              ["Mechanics", [
                ["Cadence", "15m", true],
                ["Asset", "ETH/USD", true],
                ["Stop", "ATR × 2", true],
              ]],
              ["Risk", [["Preset", "Balanced", true]]],
              ["Last eval", [["Status", "No eval yet — Run paper trade after Save", false]]],
            ].map(([title, fields]) => (
              <div className="card" key={title} style={{padding: "14px 16px"}}>
                <div className="serif" style={{fontSize: 16, marginBottom: 8}}>{title}</div>
                {fields.map(([k, v, ok], i) => (
                  <div key={i} style={{display: "flex", justifyContent: "space-between", alignItems: "center", padding: "5px 0", fontSize: 13}}>
                    <span className="mute">{k}</span>
                    <span style={{color: ok ? "var(--text)" : "var(--text-2)"}}>
                      {title === "Agents" && <span className={"dot " + (v === "drafting" ? "warn" : "gold")}/>}
                      <span className={title === "Agents" || title === "Template" || k === "Asset" || k === "Stop" || k === "Cadence" ? "mono" : ""}>{title === "Agents" ? `${k} model` : v}</span>
                      {title === "Agents" && <span style={{marginLeft: 8, fontSize: 11, color: ok === "drafting" ? "var(--warn)" : "var(--gold)"}}>{ok === "drafting" ? "drafting" : "ready"}</span>}
                    </span>
                  </div>
                ))}
              </div>
            ))}
          </div>

          <div style={{display: "flex", gap: 8, padding: "16px 0 24px"}}>
            <button className="btn ghost">Open in Inspector</button>
            <button className="btn ghost" style={{opacity: 0.5}}>Run paper trade</button>
            <button className="btn primary" style={{marginLeft: "auto"}}>Save draft</button>
          </div>
        </section>
      </main>
    </div>
  );
};
window.SetupScreen = SetupScreen;
