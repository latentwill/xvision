// Frame 3 — /marketplace · Opt-in / empty state
// Wallet not connected. Persona A always sees the nav item but lands here.

const MarketplaceOptIn = () => (
  <Frame>
    {/* Sidebar with Marketplace visible but no wallet card */}
    <SideNavOptIn/>
    <main style={{display:"flex", flexDirection:"column", minWidth:0, overflow:"hidden"}}>
      <TopStatus
        breadcrumb={[{ text:"MARKETPLACE" }, { text:"opt-in", mono:true }]}
        walletConnected={false}
      />

      <div className="dot-bg" style={{
        flex:1, minHeight:0, padding:"36px 48px 24px",
        overflow:"auto",
      }}>
        {/* Hero */}
        <div style={{
          display:"grid", gridTemplateColumns:"1fr 380px", gap:36,
          marginBottom:32,
        }}>
          <div>
            <div className="ulabel" style={{fontSize:10.5, letterSpacing:"0.22em", color:"var(--gold)", marginBottom:18}}>
              ◆ ERC-8004 · MANTLE · OPT-IN
            </div>
            <h1 style={{
              fontSize:42, fontWeight:600, letterSpacing:"-0.035em",
              lineHeight:1.05, margin:"0 0 18px", maxWidth:640,
            }}>
              On-chain reputation<br/>
              for your strategy lineages.
            </h1>
            <p style={{
              fontSize:15, lineHeight:1.55, color:"var(--text-2)",
              maxWidth:560, margin:"0 0 26px",
            }}>
              Marketplace publishes what's already provable. Sealed lineage manifests are minted
              as ERC-8004 Identity NFTs on Mantle, with counterfactual-chain Merkle roots and
              validation receipts from in-house attester agents. xvn never holds capital and the
              autoresearch loop runs identically whether you opt in or not.
            </p>
            <div style={{display:"flex", gap:10, alignItems:"center"}}>
              <Btn variant="primary" icon="wallet" style={{padding:"10px 16px", fontSize:13}}>Connect wallet</Btn>
              <Btn variant="ghost" icon="ext" style={{padding:"10px 14px"}}>Read the spec</Btn>
              <span className="mono" style={{fontSize:11, color:"var(--text-3)", marginLeft:10}}>
                no chain calls until you connect · cargo feature <span style={{color:"var(--text-2)"}}>marketplace</span> on
              </span>
            </div>
          </div>

          {/* Right: visual — stylised on-chain receipt card */}
          <ReceiptIllustration/>
        </div>

        {/* What it does — three feature cards */}
        <div style={{display:"grid", gridTemplateColumns:"1fr 1fr 1fr", gap:14, marginBottom:24}}>
          {[
            { num:"01", title:"Identity NFTs", icon:"nft",
              blurb:"One ERC-8004 Identity NFT per lineage. Manifest pinned to IPFS; CID becomes the agentURI. Variants reference by content hash inside the lineage's append-only mutation log.",
              chain:"Identity Registry" },
            { num:"02", title:"Merkle anchors", icon:"diamond",
              blurb:"Counterfactual-chain root over (parent_hash → child_hash → days_alive → trades → realized_pnl) per variant. Posted to Reputation Registry on snapshot or at lineage end.",
              chain:"Reputation Registry" },
            { num:"03", title:"Attester receipts", icon:"shield",
              blurb:"Two in-house attesters score every committed bundle. regime-verifier checks the regime claim against the trace; diversity-check confirms embedding distance to siblings.",
              chain:"Validation Registry" },
          ].map((c) => (
            <div key={c.num} style={{
              padding:"22px 22px 20px", border:"1px solid var(--border)", borderRadius:6,
              background:"transparent",
            }}>
              <div style={{display:"flex", alignItems:"center", gap:10, marginBottom:14}}>
                <div style={{
                  width:30, height:30, borderRadius:4,
                  background:"var(--gold-bg)", border:"1px solid var(--gold-soft)",
                  display:"flex", alignItems:"center", justifyContent:"center",
                }}>
                  <Icon name={c.icon} size={15} color="var(--gold)"/>
                </div>
                <span className="mono" style={{fontSize:11, color:"var(--text-3)", letterSpacing:"0.16em"}}>{c.num}</span>
                <span style={{marginLeft:"auto"}} className="mono">
                  <span style={{
                    fontSize:9.5, color:"var(--info)", letterSpacing:"0.16em",
                    padding:"2px 6px", border:"1px solid rgba(95,168,255,0.4)", borderRadius:3,
                  }}>{c.chain}</span>
                </span>
              </div>
              <h3 style={{margin:"0 0 8px", fontSize:18, fontWeight:600, letterSpacing:"-0.02em"}}>{c.title}</h3>
              <p style={{margin:0, fontSize:12.5, lineHeight:1.55, color:"var(--text-2)"}}>{c.blurb}</p>
            </div>
          ))}
        </div>

        {/* Preview strip — locked, shows what data would appear */}
        <div style={{
          padding:"14px 18px", border:"1px solid var(--border)", borderRadius:6,
          background:"transparent", position:"relative", overflow:"hidden",
        }}>
          <div style={{
            display:"flex", justifyContent:"space-between", alignItems:"center",
            marginBottom:14,
          }}>
            <div>
              <div style={{fontSize:13.5, fontWeight:600}}>What you'd see, connected</div>
              <div className="mono" style={{fontSize:11, color:"var(--text-3)", marginTop:3}}>
                preview from current session 01H8…RTZ · all reads · no writes
              </div>
            </div>
            <div style={{display:"flex", alignItems:"center", gap:7}}>
              <Icon name="lock" size={11} color="var(--text-3)"/>
              <span className="mono" style={{fontSize:10.5, color:"var(--text-3)", letterSpacing:"0.14em"}}>WALLET REQUIRED TO CONFIRM ANY ACTION</span>
            </div>
          </div>

          {/* faded preview row */}
          <div style={{opacity:0.55, pointerEvents:"none"}}>
            <table style={{width:"100%", borderCollapse:"collapse"}}>
              <thead>
                <tr>
                  {["", "Lineage", "Token", "Born", "Best Sharpe", "Anchor"].map((h, i) => (
                    <th key={i} className="ulabel" style={{
                      padding:"10px 12px", borderBottom:"1px solid var(--border-soft)",
                      textAlign:"left", fontSize:9.5, fontWeight:600,
                    }}>{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {[
                  { id:"A", n:"eth-mr",          tok:"#0042", b:"4d 18h ago", s:"+1.62", a:"Anchored 6h ago" },
                  { id:"B", n:"btc-momentum",    tok:"#0043", b:"4d 11h ago", s:"+1.31", a:"Anchored 2h ago" },
                  { id:"C", n:"btc-momentum-v3", tok:"#0048", b:"1d 03h ago", s:"+0.94", a:"Pending"         },
                  { id:"D", n:"stablecoin-flow", tok:"—",     b:"6h ago",     s:"+0.55", a:"Not yet minted"  },
                ].map((r) => (
                  <tr key={r.n}>
                    <td style={{padding:"10px 0 10px 16px", width:18}}><LineageDot id={r.id}/></td>
                    <td style={{padding:"10px 12px"}}><span className="mono" style={{fontSize:12.5}}>{r.n}</span></td>
                    <td style={{padding:"10px 12px"}}><span className="mono" style={{fontSize:11.5, color:r.tok === "—" ? "var(--text-4)" : "var(--gold)"}}>{r.tok}</span></td>
                    <td style={{padding:"10px 12px"}}><span className="mono" style={{fontSize:11, color:"var(--text-2)"}}>{r.b}</span></td>
                    <td style={{padding:"10px 12px"}}><span className="mono" style={{fontSize:12, color:"var(--gold)"}}>{r.s}</span></td>
                    <td style={{padding:"10px 12px"}}><span className="mono" style={{fontSize:11, color:"var(--text-2)"}}>{r.a}</span></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          {/* lock band overlay */}
          <div style={{
            position:"absolute", left:0, right:0, bottom:0, height:36,
            background:"linear-gradient(180deg, transparent, #000 80%)",
          }}/>
        </div>

        <div style={{
          marginTop:18, display:"flex", justifyContent:"space-between", alignItems:"center",
          padding:"8px 4px",
        }}>
          <div className="mono" style={{fontSize:11, color:"var(--text-3)"}}>
            Total v1 chain footprint over a hackathon session: <span style={{color:"var(--text-2)"}}>~20–40 transactions</span> · ~$50 on Mantle mainnet
          </div>
          <div style={{display:"flex", gap:6, alignItems:"center"}}>
            <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>Networks:</span>
            <StatusPill tone="gold" dot>MANTLE MAINNET</StatusPill>
            <StatusPill tone="neutral" dot={false}>SEPOLIA</StatusPill>
          </div>
        </div>
      </div>
    </main>
  </Frame>
);

// === Sidebar variant: shows wallet block as "not connected" CTA instead ===
const SideNavOptIn = () => {
  const items = [
    { key:"home",       label:"Home",       icon:"home" },
    { key:"strategies", label:"Strategies", icon:"chart" },
    { key:"live",       label:"Live",       icon:"play" },
    { key:"eval",       label:"Eval",       icon:"bars" },
    { key:"journal",    label:"Journal",    icon:"book" },
    { key:"marketplace",label:"Marketplace",icon:"market" },
    { key:"data",       label:"Data",       icon:"db" },
    { key:"settings",   label:"Settings",   icon:"cog" },
  ];
  return (
    <aside style={{
      background:"var(--surface-sidebar)", borderRight:"1px solid var(--border-soft)",
      display:"flex", flexDirection:"column", padding:"22px 0 14px", width:200,
    }}>
      <div style={{padding:"0 22px 24px"}}><BrandMark/></div>
      <nav style={{display:"flex", flexDirection:"column", flex:1}}>
        {items.map(i => {
          const isActive = i.key === "marketplace";
          return (
            <div key={i.key} style={{
              display:"flex", alignItems:"center", gap:12, padding:"9px 22px",
              color: isActive ? "var(--text)" : "var(--text-2)",
              borderLeft: `2px solid ${isActive ? "var(--gold)" : "transparent"}`,
              fontSize:13.5, fontWeight:500, cursor:"pointer",
            }}>
              <Icon name={i.icon} size={16} color={isActive ? "var(--gold)" : "currentColor"}/>
              <span>{i.label}</span>
            </div>
          );
        })}
      </nav>
      <div style={{
        margin:"0 14px 14px", padding:"12px",
        border:"1px dashed var(--border-strong)", borderRadius:6,
      }}>
        <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.16em", marginBottom:6}}>WALLET</div>
        <div className="mono" style={{fontSize:11, color:"var(--text-3)", marginBottom:9}}>not connected</div>
        <Btn variant="primary" dense style={{width:"100%", justifyContent:"center"}}>Connect</Btn>
      </div>
      <div style={{
        display:"flex", alignItems:"center", gap:10,
        padding:"12px 16px", borderTop:"1px solid var(--border-soft)",
      }}>
        <div style={{
          width:30, height:30, borderRadius:"50%",
          background:"var(--surface-panel)", border:"1px solid var(--border)",
          display:"flex", alignItems:"center", justifyContent:"center",
          fontSize:10.5, fontWeight:600,
        }}>AK</div>
        <div style={{flex:1, minWidth:0}}>
          <div style={{fontSize:12.5}}>Alex Kim</div>
          <div style={{fontSize:10.5, color:"var(--text-3)"}}>operator</div>
        </div>
        <Icon name="chevR" size={13} color="var(--text-3)"/>
      </div>
    </aside>
  );
};

// === Receipt illustration — a stylised NFT mint receipt ===
const ReceiptIllustration = () => (
  <div style={{
    position:"relative", height:360,
    border:"1px solid var(--border)", borderRadius:6,
    background:"linear-gradient(180deg, #0E0E0E, #060606)",
    overflow:"hidden", padding:"20px",
  }}>
    {/* corner mark */}
    <div style={{position:"absolute", top:14, right:14, display:"flex", gap:6, alignItems:"center"}}>
      <span style={{width:6, height:6, borderRadius:"50%", background:"var(--gold)"}}/>
      <span className="mono" style={{fontSize:9.5, color:"var(--gold)", letterSpacing:"0.18em"}}>MANTLE · 5000</span>
    </div>

    <div className="ulabel" style={{fontSize:10, letterSpacing:"0.22em", color:"var(--text-3)"}}>RECEIPT</div>
    <div className="mono" style={{fontSize:13, color:"var(--text-2)", marginTop:4}}>AgentRegistered</div>

    <div style={{marginTop:22, display:"flex", flexDirection:"column", gap:10}}>
      {[
        ["tokenId",     "#0043", "gold"],
        ["agentURI",    "ipfs://bafybeib4xj…q2y7l", "info"],
        ["owner",       "0xa83e…f12d4", "text"],
        ["lineage_id",  "btc-momentum", "text"],
        ["parent",      "— (seed)",     "muted"],
        ["born_at",     "2026-05-13 04:12:31Z", "text"],
      ].map(([k, v, t]) => (
        <div key={k} style={{display:"grid", gridTemplateColumns:"86px 1fr", gap:10, alignItems:"baseline"}}>
          <span className="ulabel" style={{fontSize:9.5, letterSpacing:"0.16em"}}>{k}</span>
          <span className="mono" style={{
            fontSize:11.5,
            color: t === "gold" ? "var(--gold)" : t === "info" ? "var(--info)" : t === "muted" ? "var(--text-3)" : "var(--text)",
            wordBreak:"break-all",
          }}>{v}</span>
        </div>
      ))}
    </div>

    {/* hash sig footer */}
    <div style={{
      position:"absolute", left:20, right:20, bottom:18,
      borderTop:"1px dashed var(--border-strong)", paddingTop:14,
      display:"flex", justifyContent:"space-between", alignItems:"center",
    }}>
      <span className="mono" style={{fontSize:10, color:"var(--text-3)"}}>tx 0xc0a4…f3b2</span>
      <span className="mono" style={{fontSize:10, color:"var(--gold)", letterSpacing:"0.16em"}}>◆ CONFIRMED</span>
    </div>

    {/* corner ticks */}
    {["TL","TR","BL","BR"].map((c) => (
      <span key={c} style={{
        position:"absolute", width:8, height:8,
        borderColor:"var(--gold-soft)",
        ...(c.includes("T") ? {top:6} : {bottom:6}),
        ...(c.includes("L") ? {left:6,  borderLeft:"1px solid var(--gold-soft)"}  : {right:6, borderRight:"1px solid var(--gold-soft)"}),
        ...(c.includes("T") ? {borderTop:"1px solid var(--gold-soft)"} : {borderBottom:"1px solid var(--gold-soft)"}),
      }}/>
    ))}
  </div>
);

window.MarketplaceOptIn = MarketplaceOptIn;
