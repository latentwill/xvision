// Frame 4 — /settings · Marketplace section active (wallet connected)
// Settings layout: 220px sub-nav + content. Sub-nav matches gptprompts §17/§18.

const SettingsSidebar = ({ active = "marketplace" }) => {
  const groups = [
    { label:"ACCOUNT", items:[
      { key:"account",    label:"Account" },
      { key:"appearance", label:"Appearance" },
    ]},
    { label:"CONFIG", items:[
      { key:"llm-keys",     label:"LLM keys" },
      { key:"brokers",      label:"Brokers" },
      { key:"optimizer", label:"Optimizer" },
      { key:"marketplace",  label:"Marketplace" },
      { key:"identity",     label:"Identity", indent:true },
    ]},
    { label:"RUNTIME", items:[
      { key:"daemon",    label:"Daemon" },
      { key:"telemetry", label:"Telemetry" },
    ]},
  ];
  return (
    <div style={{
      width:220, borderRight:"1px solid var(--border-soft)",
      padding:"22px 0", display:"flex", flexDirection:"column", gap:8, flexShrink:0,
      background:"#000",
    }}>
      {groups.map((g, gi) => (
        <div key={g.label}>
          <div className="ulabel" style={{
            padding:"8px 20px 6px", fontSize:9.5, letterSpacing:"0.22em",
          }}>{g.label}</div>
          {g.items.map((i) => {
            const isActive = i.key === active;
            return (
              <div key={i.key} style={{
                display:"flex", alignItems:"center", gap:8,
                padding:`8px 20px 8px ${i.indent ? "36px" : "20px"}`,
                color: isActive ? "var(--text)" : "var(--text-2)",
                borderLeft: `2px solid ${isActive ? "var(--gold)" : "transparent"}`,
                fontSize:13, fontWeight: isActive ? 600 : 500, cursor:"pointer",
              }}>
                {i.indent && <span style={{color:"var(--text-4)", marginRight:2}}>└</span>}
                <span>{i.label}</span>
              </div>
            );
          })}
          {gi < groups.length-1 && <div style={{height:1, background:"var(--border-soft)", margin:"10px 0"}}/>}
        </div>
      ))}
      <div style={{flex:1}}/>
      <div style={{height:1, background:"var(--border-soft)", margin:"4px 0"}}/>
      <div style={{
        padding:"10px 20px", color:"var(--danger)",
        fontSize:13, fontWeight:500, cursor:"pointer",
        display:"flex", alignItems:"center", gap:8,
      }}>
        <Icon name="info" size={13} color="var(--danger)"/>
        <span>Danger zone</span>
      </div>
    </div>
  );
};

// === Settings · Marketplace frame ===
const SettingsMarketplace = () => (
  <Frame>
    <SideNav active="settings"/>
    <main style={{display:"flex", flexDirection:"column", minWidth:0, overflow:"hidden"}}>
      <TopStatus breadcrumb={[
        { text:"SETTINGS" },
        { text:"marketplace", mono:true },
      ]}/>
      <div style={{flex:1, minHeight:0, display:"flex", overflow:"hidden"}}>
        <SettingsSidebar active="marketplace"/>
        <div style={{flex:1, padding:"26px 32px 22px", overflow:"auto", display:"flex", flexDirection:"column", gap:18}}>
          {/* Page header */}
          <div style={{display:"flex", justifyContent:"space-between", alignItems:"flex-end"}}>
            <div>
              <h1 style={{margin:0, fontSize:28, fontWeight:600, letterSpacing:"-0.03em", lineHeight:1.1}}>
                Marketplace
              </h1>
              <div style={{marginTop:6, fontSize:13, color:"var(--text-2)"}}>
                On-chain reputation via ERC-8004 on Mantle · opt-in
              </div>
            </div>
            <div style={{display:"flex", gap:8}}>
              <Btn variant="ghost" icon="info">What is this?</Btn>
              <Btn variant="ghost" icon="ext">View on Mantlescan</Btn>
            </div>
          </div>

          {/* Connected wallet card */}
          <WalletCard/>

          {/* Sub-section: Attester agents */}
          <SectionAttesters/>

          {/* Sub-section: Anchor preferences */}
          <SectionAnchorPrefs/>

          {/* Sub-section: Identity header strip */}
          <SectionIdentityStrip/>

          {/* Sticky footer */}
          <div style={{
            marginTop:6, padding:"14px 16px",
            border:"1px solid var(--border)", borderRadius:6,
            display:"flex", justifyContent:"space-between", alignItems:"center",
          }}>
            <div>
              <div style={{fontSize:12.5, color:"var(--text)"}}>Ready to verify settings on testnet first?</div>
              <div className="mono" style={{fontSize:11, color:"var(--text-3)", marginTop:3}}>
                runs the full mint + anchor + attest flow on Mantle Sepolia · no mainnet gas
              </div>
            </div>
            <div style={{display:"flex", gap:8}}>
              <Btn variant="danger">Disconnect wallet</Btn>
              <Btn variant="primary" icon="bolt">Test on-chain action (Sepolia)</Btn>
            </div>
          </div>
        </div>
      </div>
    </main>
  </Frame>
);

// === Wallet card ===
const WalletCard = () => (
  <div style={{
    border:"1px solid var(--border)", borderRadius:6, background:"var(--surface-elev)",
    padding:"18px 20px",
  }}>
    <div style={{display:"flex", alignItems:"center", gap:12, marginBottom:10}}>
      <div style={{
        width:34, height:34, borderRadius:5,
        background:"var(--gold-bg)", border:"1px solid var(--gold-soft)",
        display:"flex", alignItems:"center", justifyContent:"center",
      }}>
        <Icon name="wallet" size={16} color="var(--gold)"/>
      </div>
      <div style={{flex:1}}>
        <div style={{display:"flex", alignItems:"center", gap:10}}>
          <span className="mono" style={{fontSize:15, color:"var(--text)", fontWeight:500}}>0xa83e…f12d4</span>
          <span style={{padding:4, border:"1px solid var(--border)", borderRadius:3, cursor:"pointer"}}>
            <Icon name="copy" size={10} color="var(--text-3)"/>
          </span>
          <StatusPill tone="gold">MANTLE MAINNET</StatusPill>
        </div>
        <div className="mono" style={{fontSize:11.5, color:"var(--text-3)", marginTop:5}}>
          balance <span style={{color:"var(--text-2)"}}>0.42 ETH</span>
          <span style={{margin:"0 10px", color:"var(--text-4)"}}>·</span>
          chain id <span style={{color:"var(--text-2)"}}>5000</span>
          <span style={{margin:"0 10px", color:"var(--text-4)"}}>·</span>
          rpc <span style={{color:"var(--text-2)"}}>rpc.mantle.xyz</span>
          <span style={{color:"var(--info)", marginLeft:8, textDecoration:"underline dotted", cursor:"pointer"}}>edit RPC</span>
        </div>
      </div>
      <div style={{display:"flex", gap:8}}>
        <Btn variant="ghost" dense icon="branch">Switch to Sepolia</Btn>
      </div>
    </div>
  </div>
);

// === Attester agents section ===
const SectionAttesters = () => (
  <Card
    title="Attester agents"
    sub="2 active · these agents read your committed lineages and post their own validation receipts on-chain"
    right={
      <div style={{display:"flex", gap:6}}>
        <Btn variant="ghost" dense icon="ext">View on chain</Btn>
        <Btn variant="ghost" dense icon="plus">Add attester</Btn>
      </div>
    }
  >
    <div style={{padding:"4px 0"}}>
      {[
        { name:"regime-verifier", token:"#0007", blurb:"Verifies finding's regime claim against trace.",
          stats:[["endorse","27"],["question","4"],["reject","0"]], last:"2 min ago", active:true,
          optimizerNote:"signs ValidationReceipts for every committed bundle" },
        { name:"diversity-check", token:"#0008", blurb:"Confirms variant adds embedding diversity (threshold ≥ 0.18).",
          stats:[["endorse","21"],["question","6"],["reject","0"]], last:"9 min ago", active:true,
          optimizerNote:"signs ValidationReceipts when embedding distance crosses threshold" },
      ].map((a, i) => (
        <div key={a.name} style={{
          padding:"14px 18px",
          borderTop: i === 0 ? "none" : "1px solid var(--border-soft)",
          display:"grid", gridTemplateColumns:"40px 1fr 200px auto", gap:14, alignItems:"center",
        }}>
          <div style={{
            width:36, height:36, borderRadius:5,
            background:"var(--gold-bg)", border:"1px solid var(--gold-soft)",
            display:"flex", alignItems:"center", justifyContent:"center",
          }}>
            <Icon name="shield" size={16} color="var(--gold)"/>
          </div>
          <div>
            <div style={{display:"flex", alignItems:"center", gap:10}}>
              <span style={{fontSize:14, fontWeight:600}}>{a.name}</span>
              <span className="mono" style={{fontSize:11, color:"var(--gold)"}}>NFT {a.token}</span>
              <StatusPill tone="gold" pulse>ACTIVE</StatusPill>
            </div>
            <div style={{fontSize:12, color:"var(--text-2)", marginTop:4}}>{a.blurb}</div>
            <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginTop:4}}>{a.optimizerNote}</div>
          </div>
          <div style={{display:"flex", gap:14}}>
            {a.stats.map(([k, v]) => (
              <div key={k}>
                <div className="ulabel" style={{fontSize:9, letterSpacing:"0.14em"}}>{k}</div>
                <div className="mono" style={{
                  fontSize:15, marginTop:2,
                  color: k === "endorse" ? "var(--gold)" : k === "question" ? "var(--warn)" : k === "reject" ? "var(--danger)" : "var(--text)",
                }}>{v}</div>
              </div>
            ))}
          </div>
          <div style={{display:"flex", flexDirection:"column", gap:6, alignItems:"flex-end"}}>
            <span className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>last {a.last}</span>
            <div style={{display:"flex", gap:6}}>
              <Btn variant="ghost" dense>Pause</Btn>
              <Btn variant="ghost" dense icon="ext">On chain</Btn>
            </div>
          </div>
        </div>
      ))}
    </div>
  </Card>
);

// === Anchor preferences section ===
const SectionAnchorPrefs = () => {
  const modes = [
    { id:"on-demand", label:"On-demand",          desc:"Operator chooses when to post each anchor",          active:true },
    { id:"per-cycle", label:"After every cycle",  desc:"Anchors after each evening cycle · ≈3× gas",         active:false, warn:true },
    { id:"session",   label:"At session end only", desc:"One LineageEnd anchor per lineage at session close", active:false },
  ];
  return (
    <Card
      title="Anchor preferences"
      sub="When and how lineages reach Mantle"
    >
      <div style={{padding:"16px 18px", display:"grid", gridTemplateColumns:"1fr 280px", gap:32, alignItems:"start"}}>
        {/* Anchor mode */}
        <div>
          <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em", marginBottom:10}}>ANCHOR MODE</div>
          <div style={{display:"flex", flexDirection:"column", gap:8}}>
            {modes.map((m) => (
              <label key={m.id} style={{
                display:"flex", alignItems:"flex-start", gap:12,
                padding:"11px 14px",
                border:`1px solid ${m.active ? "var(--gold-soft)" : "var(--border)"}`,
                background: m.active ? "var(--gold-bg)" : "transparent",
                borderRadius:5, cursor:"pointer",
              }}>
                <span style={{
                  width:14, height:14, borderRadius:"50%",
                  border: `1.5px solid ${m.active ? "var(--gold)" : "var(--text-3)"}`,
                  marginTop:3, flexShrink:0,
                  background: m.active ? "radial-gradient(circle, var(--gold) 0 40%, transparent 41%)" : "transparent",
                }}/>
                <div style={{flex:1}}>
                  <div style={{display:"flex", alignItems:"center", gap:8}}>
                    <span style={{fontSize:13, fontWeight:600, color: m.active ? "var(--gold)" : "var(--text)"}}>{m.label}</span>
                    {m.warn && <span className="mono" style={{fontSize:9.5, color:"var(--warn)", letterSpacing:"0.14em", padding:"1px 6px", border:"1px solid var(--warn)", borderRadius:3}}>HIGHER GAS</span>}
                  </div>
                  <div style={{fontSize:11.5, color:"var(--text-3)", marginTop:3}}>{m.desc}</div>
                </div>
              </label>
            ))}
          </div>
        </div>

        {/* IPFS pin provider */}
        <div>
          <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em", marginBottom:10}}>IPFS PIN PROVIDER</div>
          <div style={{
            display:"flex", alignItems:"center", justifyContent:"space-between",
            padding:"11px 14px", border:"1px solid var(--border)", borderRadius:5,
            background:"var(--surface-elev)",
          }}>
            <span style={{fontSize:13}}>Pinata</span>
            <Icon name="chevD" size={12} color="var(--text-3)"/>
          </div>
          <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginTop:8, lineHeight:1.5}}>
            options · Pinata · Web3.Storage · Filebase · Self-hosted (operator URL)
          </div>

          <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em", marginTop:18, marginBottom:8}}>OPERATOR KEY SEPARATION</div>
          <div style={{padding:"10px 12px", border:"1px solid var(--border)", borderRadius:5}}>
            <div style={{display:"flex", justifyContent:"space-between", alignItems:"center"}}>
              <span style={{fontSize:12, color:"var(--text-2)"}}>Optimizer signing key</span>
              <span className="mono" style={{fontSize:11, color:"var(--text)"}}>ed25519:7f2b…91c4</span>
            </div>
            <div style={{display:"flex", justifyContent:"space-between", alignItems:"center", marginTop:6}}>
              <span style={{fontSize:12, color:"var(--text-2)"}}>On-chain wallet</span>
              <span className="mono" style={{fontSize:11, color:"var(--gold)"}}>0xa83e…f12d4</span>
            </div>
            <div className="mono" style={{fontSize:10, color:"var(--text-3)", marginTop:8, letterSpacing:"0.04em"}}>
              keys deliberately distinct · seals are off-chain, txs are on-chain
            </div>
          </div>
        </div>
      </div>

      {/* Gas estimate footer */}
      <div style={{
        padding:"10px 18px", borderTop:"1px solid var(--border-soft)",
        display:"flex", justifyContent:"space-between", alignItems:"center",
      }}>
        <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>
          <Icon name="gas" size={11} color="var(--text-3)" sw={1.5}/>
          {" "}estimated gas this session at current settings · <span style={{color:"var(--warn)"}}>~0.014 ETH · ~$42</span>
        </span>
        <Btn variant="ghost" dense>Reset to defaults</Btn>
      </div>
    </Card>
  );
};

// === Identity strip — link out to /settings/marketplace/identity ===
const SectionIdentityStrip = () => (
  <div style={{
    padding:"16px 20px", border:"1px solid var(--border)", borderRadius:6,
    display:"flex", justifyContent:"space-between", alignItems:"center",
    background:"transparent",
  }}>
    <div style={{display:"flex", alignItems:"center", gap:14}}>
      <div style={{
        width:32, height:32, borderRadius:5,
        background:"transparent", border:"1px solid var(--border-strong)",
        display:"flex", alignItems:"center", justifyContent:"center",
      }}>
        <Icon name="nft" size={15} color="var(--gold)"/>
      </div>
      <div>
        <div style={{fontSize:13.5, fontWeight:600}}>Identity (ERC-8004)</div>
        <div className="mono" style={{fontSize:11, color:"var(--text-3)", marginTop:3}}>
          operator NFT <span style={{color:"var(--gold)"}}>#0001</span>
          <span style={{color:"var(--text-4)", margin:"0 8px"}}>·</span>
          2 attester NFTs <span style={{color:"var(--gold)"}}>#0007</span>{" "}<span style={{color:"var(--gold)"}}>#0008</span>
        </div>
      </div>
    </div>
    <Btn variant="ghost" icon="chevR">Manage identity</Btn>
  </div>
);

window.SettingsMarketplace = SettingsMarketplace;
window.SettingsSidebar = SettingsSidebar;
