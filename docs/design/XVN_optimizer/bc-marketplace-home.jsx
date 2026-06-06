// Frame 1 — /marketplace · Mantle mainnet (operator dashboard)
// Four-panel 2×2 grid: Lineages, Attestations, Anchor history, Operator actions

const LINEAGES = [
  { id:"A", name:"eth-mr",          token:"#0042", parent:null,        born:"4d 18h ago", sharpe:"+1.62", status:"anchored", lastAnchor:"6h ago" },
  { id:"B", name:"btc-momentum",    token:"#0043", parent:null,        born:"4d 11h ago", sharpe:"+1.31", status:"anchored", lastAnchor:"2h ago" },
  { id:"C", name:"btc-momentum-v3", token:"#0048", parent:"btc-momentum", born:"1d 03h ago", sharpe:"+0.94", status:"pending",  lastAnchor:null   },
  { id:"D", name:"stablecoin-flow", token:null,    parent:null,        born:"6h ago",     sharpe:"+0.55", status:"unminted", lastAnchor:null   },
];

const ATTESTERS = [
  { name:"regime-verifier",  token:"#0007", endorse:27, question:4, reject:0, last:"2 min ago",
    blurb:"Verifies finding's regime claim against trace" },
  { name:"diversity-check",  token:"#0008", endorse:21, question:6, reject:0, last:"9 min ago",
    blurb:"Confirms variant adds embedding diversity" },
];

const VERDICTS = [
  { attester:"regime-verifier", verdict:"ENDORSE",  target:"btc-momentum-v3.2", t:"2 min ago",
    rationale:"Trending claim verified against trace.", grouped:true },
  { attester:"diversity-check", verdict:"QUESTION", target:"btc-momentum-v3.2", t:"2 min ago",
    rationale:"Embedding distance to v3.1 < 0.18 threshold.", grouped:true },
  { attester:"regime-verifier", verdict:"ENDORSE",  target:"eth-mr-v4.1",       t:"18 min ago", rationale:"Mean-reversion regime + 92% trace overlap." },
  { attester:"diversity-check", verdict:"ENDORSE",  target:"eth-mr-v4.1",       t:"18 min ago", rationale:"+0.34 embedding distance to siblings." },
];

const ANCHOR_HISTORY = [
  { kind:"merkle", target:"eth-mr (lineage A)",          tx:"0x4f8a…dc11", t:"14 min ago",  gas:"0.0024 ETH" },
  { kind:"mint",   target:"btc-momentum-v3 (lineage C)", tx:"0x91bc…aa72", t:"1h 02m ago",  gas:"0.0011 ETH" },
  { kind:"merkle", target:"btc-momentum (lineage B)",    tx:"0x2e1d…44a9", t:"2h 18m ago",  gas:"0.0024 ETH" },
  { kind:"mint",   target:"btc-momentum (lineage B)",    tx:"0xc0a4…f3b2", t:"4d 11h ago",  gas:"0.0011 ETH" },
  { kind:"mint",   target:"eth-mr (lineage A)",          tx:"0x77f9…1ed8", t:"4d 18h ago",  gas:"0.0011 ETH" },
  { kind:"commit", target:"SessionCommitment 01H8…RTZ",  tx:"0x4f8a…ee01", t:"4d 18h ago",  gas:"0.0008 ETH" },
];

const OPERATOR_ACTIONS = [
  { title:"Mint missing NFTs",   blurb:"1 lineage not yet minted · stablecoin-flow",            cost:"~0.001 ETH · ~$3",  cta:"Mint now",   primary:true },
  { title:"Anchor a lineage",    blurb:"Post counterfactual-chain Merkle root for one lineage", cost:"~0.0024 ETH · ~$7", cta:"Choose…",    primary:false },
  { title:"Anchor all final",    blurb:"Hackathon-end: LineageEnd receipt for every lineage",   cost:"~0.014 ETH · ~$42", cta:"Anchor all", primary:true,  lock:true },
  { title:"Run attesters",       blurb:"Force both attesters to score recent unscored bundles", cost:"~0.0008 ETH · ~$2", cta:"Run now",    primary:false },
];

// === Marketplace home component ===
const MarketplaceHome = () => (
  <Frame>
    <SideNav active="marketplace"/>
    <main style={{display:"flex", flexDirection:"column", minWidth:0, overflow:"hidden"}}>
      <TopStatus breadcrumb={[
        { text:"MARKETPLACE" },
        { text:"mantle mainnet", mono:true },
      ]}/>

      {/* Page header strip */}
      <div style={{
        padding:"22px 28px 18px", borderBottom:"1px solid var(--border)",
        display:"flex", justifyContent:"space-between", alignItems:"flex-end", gap:24,
      }}>
        <div style={{minWidth:0}}>
          <h1 style={{
            margin:0, fontSize:30, fontWeight:600, letterSpacing:"-0.03em", lineHeight:1.1,
          }}>Marketplace</h1>
          <div className="mono" style={{
            marginTop:8, fontSize:12, color:"var(--text-3)", letterSpacing:"0.01em",
          }}>
            <span style={{color:"var(--gold)"}}>● mantle mainnet</span>
            <span style={{margin:"0 10px", color:"var(--text-4)"}}>·</span>
            <span><span style={{color:"var(--text-2)"}}>4</span> lineages on chain</span>
            <span style={{margin:"0 10px", color:"var(--text-4)"}}>·</span>
            <span><span style={{color:"var(--text-2)"}}>9</span> NFTs minted</span>
            <span style={{margin:"0 10px", color:"var(--text-4)"}}>·</span>
            <span><span style={{color:"var(--text-2)"}}>18</span> attestations posted</span>
            <span style={{margin:"0 10px", color:"var(--text-4)"}}>·</span>
            <span style={{color:"var(--text-3)"}}>session 01H8…RTZ</span>
          </div>
        </div>
        <div style={{display:"flex", gap:8, alignItems:"center"}}>
          <Btn variant="ghost" icon="branch">Switch to Sepolia</Btn>
          <Btn variant="ghost" icon="ext">View on Mantlescan</Btn>
          <Btn variant="primary" lock>Anchor all final</Btn>
        </div>
      </div>

      {/* 2×2 panel grid */}
      <div style={{
        flex:1, minHeight:0, padding:18,
        display:"grid", gridTemplateColumns:"1fr 1fr", gridTemplateRows:"1fr 1fr", gap:14,
      }}>
        <LineagesPanel/>
        <AttestationsPanel/>
        <AnchorHistoryPanel/>
        <OperatorActionsPanel/>
      </div>
    </main>
  </Frame>
);

// === Panel 1: Lineages on chain ===
const LineagesPanel = () => (
  <Card
    title="Lineages on chain"
    sub="4 active · 1 unminted · last mint tx 0x4f8a…dc11 · 14m ago"
    right={<Btn variant="ghost" dense icon="search">Filter</Btn>}
  >
    <table style={{width:"100%", borderCollapse:"collapse"}}>
      <thead>
        <tr>
          {["", "Lineage", "Token", "Parent", "Born", "Sharpe", "Anchor", ""].map((h, i) => (
            <th key={i} className="ulabel" style={{
              padding:"10px 12px", borderBottom:"1px solid var(--border-soft)",
              textAlign: i === 5 ? "right" : "left", fontSize:9.5, fontWeight:600,
              whiteSpace:"nowrap",
            }}>{h}</th>
          ))}
        </tr>
      </thead>
      <tbody>
        {LINEAGES.map((l) => (
          <tr key={l.name} style={{cursor:"pointer"}}>
            <td style={{padding:"11px 0 11px 16px", width:18}}><LineageDot id={l.id}/></td>
            <td style={{padding:"11px 12px 11px 8px"}}>
              <span className="mono" style={{fontSize:12.5, color:"var(--text)"}}>{l.name}</span>
            </td>
            <td style={{padding:"11px 12px"}}>
              {l.token
                ? <span className="mono" style={{fontSize:12, color:"var(--gold)"}}>{l.token}</span>
                : <span className="mono" style={{fontSize:11, color:"var(--text-4)"}}>—</span>}
            </td>
            <td style={{padding:"11px 12px"}}>
              {l.parent
                ? <span style={{
                    display:"inline-flex", alignItems:"center", gap:5,
                    padding:"2px 7px", border:"1px solid var(--border-strong)", borderRadius:3,
                    fontFamily:"'Geist Mono', monospace", fontSize:10.5, color:"var(--text-2)",
                  }}>
                    <Icon name="branch" size={9} color="var(--text-3)"/>{l.parent}
                  </span>
                : <span className="mono" style={{fontSize:11, color:"var(--text-4)"}}>—</span>}
            </td>
            <td style={{padding:"11px 12px"}}>
              <span className="mono" style={{fontSize:11.5, color:"var(--text-2)"}}>{l.born}</span>
            </td>
            <td style={{padding:"11px 12px", textAlign:"right"}}>
              <span className="mono" style={{fontSize:12.5, color:"var(--gold)"}}>{l.sharpe}</span>
            </td>
            <td style={{padding:"11px 12px"}}>
              {l.status === "anchored" && (
                <span style={{display:"inline-flex", alignItems:"center", gap:6}}>
                  <span style={{width:6, height:6, borderRadius:"50%", background:"var(--gold)"}}/>
                  <span className="mono" style={{fontSize:11, color:"var(--text-2)"}}>Anchored {l.lastAnchor}</span>
                </span>
              )}
              {l.status === "pending" && (
                <span style={{display:"inline-flex", alignItems:"center", gap:6}}>
                  <span className="pulse" style={{width:6, height:6, borderRadius:"50%", background:"var(--warn)"}}/>
                  <span className="mono" style={{fontSize:11, color:"var(--warn)"}}>Pending anchor</span>
                </span>
              )}
              {l.status === "unminted" && (
                <span style={{display:"inline-flex", alignItems:"center", gap:6}}>
                  <span style={{width:6, height:6, borderRadius:"50%", border:"1px solid var(--text-3)"}}/>
                  <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>Not yet minted</span>
                </span>
              )}
            </td>
            <td style={{padding:"11px 16px 11px 4px", textAlign:"right", color:"var(--text-3)"}}>
              <Icon name="ext" size={12}/>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  </Card>
);

// === Panel 2: Attestations (in-house) ===
const AttestationsPanel = () => (
  <Card
    title="Attestations · in-house"
    sub="2 attesters operated by xvision · v1 · external participation in v2"
    right={<Btn variant="ghost" dense>Add attester (advanced)</Btn>}
  >
    {/* Attester cards */}
    <div style={{padding:"12px 14px", display:"flex", flexDirection:"column", gap:8,
                 borderBottom:"1px solid var(--border-soft)"}}>
      {ATTESTERS.map((a) => (
        <div key={a.name} style={{
          display:"flex", alignItems:"center", gap:14,
          padding:"10px 12px", border:"1px solid var(--border)", borderRadius:5,
          background:"var(--surface-elev)",
        }}>
          <div style={{
            width:32, height:32, borderRadius:5,
            background:"var(--gold-bg)", border:"1px solid var(--gold-soft)",
            display:"flex", alignItems:"center", justifyContent:"center",
          }}>
            <Icon name="shield" size={15} color="var(--gold)"/>
          </div>
          <div style={{flex:1, minWidth:0}}>
            <div style={{display:"flex", alignItems:"center", gap:8}}>
              <span style={{fontSize:13, fontWeight:600}}>{a.name}</span>
              <span className="mono" style={{fontSize:11, color:"var(--gold)"}}>NFT {a.token}</span>
              <span style={{display:"inline-flex", alignItems:"center", gap:5, marginLeft:"auto"}}>
                <span className="pulse" style={{width:5, height:5, borderRadius:"50%", background:"var(--gold)"}}/>
                <span className="mono" style={{fontSize:10, color:"var(--gold)", letterSpacing:"0.16em"}}>ACTIVE</span>
              </span>
            </div>
            <div className="mono" style={{fontSize:11, color:"var(--text-3)", marginTop:3}}>
              {a.blurb} · <span style={{color:"var(--gold)"}}>{a.endorse} endorse</span> · {a.question} question · {a.reject} reject · last action {a.last}
            </div>
          </div>
        </div>
      ))}
    </div>

    {/* Verdict feed */}
    <div style={{padding:"10px 14px 12px"}}>
      <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em", marginBottom:8}}>RECENT VERDICTS</div>
      <div style={{display:"flex", flexDirection:"column"}}>
        {VERDICTS.map((v, i) => {
          const tone = v.verdict === "ENDORSE" ? "gold"
                     : v.verdict === "QUESTION" ? "warn" : "danger";
          const toneFg = tone === "gold" ? "var(--gold)"
                       : tone === "warn" ? "var(--warn)" : "var(--danger)";
          const isGroupedPair = v.grouped && VERDICTS[i+1] && VERDICTS[i+1].grouped;
          return (
            <div key={i} style={{
              display:"flex", alignItems:"flex-start", gap:10,
              padding:"7px 0",
              borderBottom: i < VERDICTS.length-1 ? "1px solid var(--border-soft)" : "none",
              position:"relative",
            }}>
              {/* Verdict pill */}
              <div style={{
                minWidth:80, display:"inline-flex", alignItems:"center", gap:5,
                padding:"3px 7px", border:`1px solid ${toneFg}`, borderRadius:3,
                background:`${toneFg.replace(")", ", 0.10)").replace("var(--", "rgba(0,230,118")}`,
              }}>
                <span style={{width:5, height:5, borderRadius:"50%", background:toneFg}}/>
                <span className="mono" style={{fontSize:9.5, color:toneFg, letterSpacing:"0.14em", fontWeight:600}}>{v.verdict}</span>
              </div>
              <div style={{flex:1, minWidth:0}}>
                <div className="mono" style={{fontSize:11.5}}>
                  <span style={{color:"var(--text)"}}>{v.attester}</span>
                  <span style={{color:"var(--text-4)", margin:"0 8px"}}>·</span>
                  <span style={{color:"var(--text-2)"}}>{v.target}</span>
                  <span style={{color:"var(--text-4)", margin:"0 8px"}}>·</span>
                  <span style={{color:"var(--text-3)"}}>{v.t}</span>
                </div>
                <div style={{fontSize:11, color:"var(--text-3)", marginTop:3, fontStyle:"normal"}}>
                  {v.rationale}
                </div>
              </div>
            </div>
          );
        })}
      </div>
      {/* disagreement note */}
      <div style={{
        marginTop:10, padding:"6px 10px", border:"1px dashed var(--warn)",
        borderRadius:3, background:"rgba(255,176,32,0.06)",
        display:"flex", alignItems:"center", gap:8,
      }}>
        <Icon name="info" size={12} color="var(--warn)"/>
        <span className="mono" style={{fontSize:10.5, color:"var(--warn)", letterSpacing:"0.04em"}}>
          DISAGREEMENT on btc-momentum-v3.2 · regime endorses · diversity questions
        </span>
      </div>
    </div>
  </Card>
);

// === Panel 3: Anchor history ===
const AnchorHistoryPanel = () => {
  const glyph = { mint:"◆", merkle:"◇", commit:"✦" };
  const tone  = { mint:"var(--gold)", merkle:"var(--info)", commit:"var(--text-2)" };
  return (
    <Card
      title="Anchor history"
      sub="6 events · total session gas 0.014 ETH · ~$42"
      right={<Btn variant="ghost" dense icon="ext">Mantlescan</Btn>}
    >
      <div style={{padding:"6px 0"}}>
        {ANCHOR_HISTORY.map((e, i) => (
          <div key={i} style={{
            display:"grid", gridTemplateColumns:"22px 110px 1fr auto auto",
            alignItems:"center", gap:12, padding:"9px 16px",
            borderBottom: i < ANCHOR_HISTORY.length-1 ? "1px solid var(--border-soft)" : "none",
          }}>
            <span style={{fontSize:14, color:tone[e.kind], textAlign:"center"}}>{glyph[e.kind]}</span>
            <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>{e.t}</span>
            <span className="mono" style={{fontSize:11.5, color:"var(--text-2)", overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap"}}>
              <span className="ulabel" style={{fontSize:9, color:tone[e.kind], marginRight:8, letterSpacing:"0.18em"}}>
                {e.kind.toUpperCase()}
              </span>
              {e.target}
            </span>
            <TxChip hash={e.tx}/>
            <span className="mono" style={{fontSize:11, color:"var(--text-3)", minWidth:80, textAlign:"right"}}>{e.gas}</span>
          </div>
        ))}
      </div>
    </Card>
  );
};

// === Panel 4: Operator actions ===
const OperatorActionsPanel = () => (
  <Card
    title="Operator actions"
    sub="Each action settles on-chain · gas estimates below"
  >
    <div style={{padding:"6px 0"}}>
      {OPERATOR_ACTIONS.map((a, i) => (
        <div key={i} style={{
          display:"flex", alignItems:"center", gap:14, padding:"14px 16px",
          borderBottom: i < OPERATOR_ACTIONS.length-1 ? "1px solid var(--border-soft)" : "none",
        }}>
          <div style={{
            width:34, height:34, border:"1px solid var(--border-strong)",
            borderRadius:5, background:"var(--surface-elev)",
            display:"flex", alignItems:"center", justifyContent:"center",
            color:"var(--gold)", flexShrink:0,
          }}>
            <Icon name={a.lock ? "lock" : "bolt"} size={15} color="var(--gold)"/>
          </div>
          <div style={{flex:1, minWidth:0}}>
            <div style={{fontSize:13, fontWeight:600, color:"var(--text)"}}>{a.title}</div>
            <div className="mono" style={{fontSize:11, color:"var(--text-3)", marginTop:3}}>
              {a.blurb}
              <span style={{color:"var(--text-4)", margin:"0 7px"}}>·</span>
              <span style={{color:"var(--warn)"}}>est. {a.cost}</span>
            </div>
          </div>
          <Btn variant={a.primary ? "primary" : "ghost"} lock={a.lock}>{a.cta}</Btn>
        </div>
      ))}
    </div>
  </Card>
);

window.MarketplaceHome = MarketplaceHome;
