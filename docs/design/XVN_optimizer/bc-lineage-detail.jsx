// Frame 2 — /marketplace/<lineage_id> · Lineage detail
// Shows: lineage NFT, variant ancestry tree, attestation verdicts,
// Merkle anchor visualization, on-chain receipts list.

const LINEAGE = {
  name: "btc-momentum",
  token: "#0043",
  parent: null,
  born: "2026-05-13 04:12Z",
  bornRel: "4d 11h ago",
  sharpe: "+1.31",
  status: "anchored",
  lastAnchor: "2h 18m ago",
  owner: "0xa83e…f12d4",
  manifestCid: "bafybeib4xj…q2y7l",
  sessionId: "01H8…RTZ",
  operatorSig: "ed25519:7f2b1ad…91c4",
  mintTx: "0xc0a4…f3b2",
  merkleTx: "0x2e1d…44a9",
  merkleRoot: "0x9a8f2c…04ed3b",
};

const VARIANTS = [
  { id:"v1.0", born:"4d 11h ago", hash:"7f2b1ad…91c4", sharpe:"+0.88", parent:null,   kind:"seed",   trades:142, pnl:"+$2,140" },
  { id:"v1.1", born:"3d 22h ago", hash:"c4d50f8…bb13", sharpe:"+1.04", parent:"v1.0", kind:"mutate", trades:118, pnl:"+$1,820" },
  { id:"v2.0", born:"3d 09h ago", hash:"9a8b2f1…02e7", sharpe:"+1.18", parent:"v1.1", kind:"mutate", trades:201, pnl:"+$3,610" },
  { id:"v2.1", born:"2d 17h ago", hash:"f12d8a4…d8c2", sharpe:"+1.22", parent:"v2.0", kind:"mutate", trades:154, pnl:"+$2,290" },
  { id:"v3.0", born:"1d 11h ago", hash:"b1e90c7…7a48", sharpe:"+1.31", parent:"v2.1", kind:"mutate", trades:97,  pnl:"+$1,640", current:true },
  { id:"v3.1", born:"06h ago",    hash:"3d72e5b…1ac9", sharpe:"+0.84", parent:"v3.0", kind:"sibling", trades:42, pnl:"+$420",   diverged:true },
];

const LINEAGE_ATTESTATIONS = [
  { attester:"regime-verifier",  token:"#0007", target:"v3.0", verdict:"ENDORSE",  t:"1h ago",  tx:"0x1aa4…b201", rationale:"Trending claim verified — 92% trace overlap." },
  { attester:"diversity-check",  token:"#0008", target:"v3.0", verdict:"ENDORSE",  t:"1h ago",  tx:"0x55cd…ff19", rationale:"+0.31 embedding distance to v2.1." },
  { attester:"regime-verifier",  token:"#0007", target:"v3.1", verdict:"ENDORSE",  t:"4h ago",  tx:"0x9b3e…a811", rationale:"Regime band matches." },
  { attester:"diversity-check",  token:"#0008", target:"v3.1", verdict:"QUESTION", t:"4h ago",  tx:"0xa07f…ee43", rationale:"Embedding distance to v3.0 < 0.18 threshold." },
  { attester:"regime-verifier",  token:"#0007", target:"v2.1", verdict:"ENDORSE",  t:"2d ago",  tx:"0x4f80…d122", rationale:"Trending claim verified." },
];

const RECEIPTS = [
  { kind:"Mint",            label:"Identity NFT minted",                tx:"0xc0a4…f3b2", t:"4d 11h ago", gas:"0.0011 ETH" },
  { kind:"Mint",            label:"Variant v1.1 referenced in manifest", tx:null,         t:"3d 22h ago", gas:null         },
  { kind:"Validation",      label:"regime-verifier ENDORSE v2.0",        tx:"0x6e21…aa07", t:"3d 06h ago", gas:"0.0006 ETH" },
  { kind:"Validation",      label:"diversity-check ENDORSE v2.0",        tx:"0x8a99…12d3", t:"3d 06h ago", gas:"0.0006 ETH" },
  { kind:"Merkle",          label:"Snapshot · receipt_kind=Snapshot",    tx:"0x2e1d…44a9", t:"2h 18m ago", gas:"0.0024 ETH" },
];

const LineageDetail = () => (
  <Frame>
    <SideNav active="marketplace"/>
    <main style={{display:"flex", flexDirection:"column", minWidth:0, overflow:"hidden"}}>
      <TopStatus breadcrumb={[
        { text:"MARKETPLACE" },
        { text:"lineage", mono:false },
        { text:LINEAGE.name, mono:true },
      ]}/>

      {/* Page header strip */}
      <div style={{
        padding:"22px 28px 18px", borderBottom:"1px solid var(--border)",
        display:"flex", justifyContent:"space-between", alignItems:"flex-end", gap:24,
      }}>
        <div style={{minWidth:0}}>
          <div style={{display:"flex", alignItems:"center", gap:14, marginBottom:8}}>
            <LineageDot id="B" size={12}/>
            <h1 style={{margin:0, fontSize:30, fontWeight:600, letterSpacing:"-0.03em", lineHeight:1, fontFamily:"'Geist Mono', monospace"}}>
              {LINEAGE.name}
            </h1>
            <span style={{
              padding:"4px 10px", border:"1px solid var(--gold-soft)",
              background:"var(--gold-bg)", borderRadius:3,
              fontFamily:"'Geist Mono', monospace", fontSize:12, fontWeight:600,
              color:"var(--gold)",
            }}>NFT {LINEAGE.token}</span>
            <StatusPill tone="gold">ANCHORED · {LINEAGE.lastAnchor.toUpperCase()}</StatusPill>
          </div>
          <div className="mono" style={{fontSize:12, color:"var(--text-3)"}}>
            born {LINEAGE.born}
            <span style={{color:"var(--text-4)", margin:"0 10px"}}>·</span>
            <span>session <span style={{color:"var(--text-2)"}}>{LINEAGE.sessionId}</span></span>
            <span style={{color:"var(--text-4)", margin:"0 10px"}}>·</span>
            <span>owner <span style={{color:"var(--text-2)"}}>{LINEAGE.owner}</span></span>
            <span style={{color:"var(--text-4)", margin:"0 10px"}}>·</span>
            <span>best Sharpe <span style={{color:"var(--gold)"}}>{LINEAGE.sharpe}</span></span>
          </div>
        </div>
        <div style={{display:"flex", gap:8, alignItems:"center"}}>
          <Btn variant="ghost" icon="ext">View NFT</Btn>
          <Btn variant="ghost" icon="ext">View IPFS manifest</Btn>
          <Btn variant="primary" lock>Anchor snapshot</Btn>
        </div>
      </div>

      {/* Body grid: 8/4 columns */}
      <div style={{
        flex:1, minHeight:0, padding:18,
        display:"grid", gridTemplateColumns:"1fr 360px", gap:14, overflow:"hidden",
      }}>
        <div style={{display:"flex", flexDirection:"column", gap:14, minHeight:0, overflow:"hidden"}}>
          <VariantTreeCard/>
          <div style={{display:"grid", gridTemplateColumns:"1fr 1fr", gap:14, minHeight:0}}>
            <AttestationVerdictsCard/>
            <MerkleProofCard/>
          </div>
        </div>
        <div style={{display:"flex", flexDirection:"column", gap:14, minHeight:0, overflow:"hidden"}}>
          <ManifestCard/>
          <ReceiptsCard/>
        </div>
      </div>
    </main>
  </Frame>
);

// === Variant tree (left-to-right horizontal ancestry) ===
const VariantTreeCard = () => {
  // Compute x positions per generation (depth) and y positions per sibling.
  // Linear chain: v1.0 → v1.1 → v2.0 → v2.1 → v3.0 ; v3.1 is sibling of v3.0
  const positions = {
    "v1.0": { x: 60,  y: 70 },
    "v1.1": { x: 200, y: 70 },
    "v2.0": { x: 340, y: 70 },
    "v2.1": { x: 480, y: 70 },
    "v3.0": { x: 620, y: 70 },
    "v3.1": { x: 620, y: 150 },
  };
  return (
    <Card
      title="Variant ancestry"
      sub={`${VARIANTS.length} variants · referenced by content hash in lineage manifest`}
      right={
        <div style={{display:"flex", gap:6, alignItems:"center"}}>
          <Btn variant="ghost" dense icon="search">Find</Btn>
          <Btn variant="chip" dense icon="diamond">Compute Merkle</Btn>
        </div>
      }
    >
      <div style={{position:"relative", height:200, overflow:"hidden"}}>
        <svg width="100%" height="200" style={{display:"block"}}>
          <defs>
            <linearGradient id="trunk" x1="0" y1="0" x2="1" y2="0">
              <stop offset="0" stopColor="#5FA8FF" stopOpacity="0.6"/>
              <stop offset="1" stopColor="#00E676" stopOpacity="0.8"/>
            </linearGradient>
          </defs>
          {/* main trunk */}
          {[["v1.0","v1.1"],["v1.1","v2.0"],["v2.0","v2.1"],["v2.1","v3.0"]].map(([a,b]) => (
            <line key={`${a}-${b}`}
              x1={positions[a].x+18} y1={positions[a].y}
              x2={positions[b].x-18} y2={positions[b].y}
              stroke="url(#trunk)" strokeWidth="1.2"/>
          ))}
          {/* fork to v3.1 */}
          <path d={`M ${positions["v2.1"].x+18} ${positions["v2.1"].y} C ${positions["v3.1"].x-60} ${positions["v3.1"].y}, ${positions["v3.1"].x-60} ${positions["v3.1"].y}, ${positions["v3.1"].x-18} ${positions["v3.1"].y}`}
            stroke="var(--warn)" strokeWidth="1" strokeDasharray="3 3" fill="none" opacity="0.7"/>
        </svg>

        {/* variant nodes */}
        {VARIANTS.map((v) => {
          const p = positions[v.id];
          const isCurrent = v.current;
          const isDiverged = v.diverged;
          const fg = isCurrent ? "var(--gold)" : isDiverged ? "var(--warn)" : "var(--text-2)";
          const bd = isCurrent ? "var(--gold)" : isDiverged ? "var(--warn)" : "var(--border-strong)";
          const bg = isCurrent ? "var(--gold-bg)" : isDiverged ? "rgba(255,176,32,0.08)" : "var(--surface-elev)";
          return (
            <div key={v.id} style={{
              position:"absolute", left:p.x-36, top:p.y-22, width:72, padding:"5px 8px",
              border:`1px solid ${bd}`, background:bg, borderRadius:4, textAlign:"center",
            }}>
              <div className="mono" style={{fontSize:11.5, color:fg, fontWeight:600}}>{v.id}</div>
              <div className="mono" style={{fontSize:9.5, color:"var(--text-3)", marginTop:2}}>{v.sharpe}</div>
            </div>
          );
        })}

        {/* Legend */}
        <div style={{position:"absolute", left:14, bottom:10, display:"flex", gap:14, alignItems:"center"}}>
          <span style={{display:"inline-flex", alignItems:"center", gap:5}}>
            <span style={{width:9, height:9, border:"1px solid var(--gold)", background:"var(--gold-bg)", borderRadius:2}}/>
            <span className="mono" style={{fontSize:10, color:"var(--text-3)", letterSpacing:"0.14em"}}>CURRENT</span>
          </span>
          <span style={{display:"inline-flex", alignItems:"center", gap:5}}>
            <span style={{width:9, height:9, border:"1px solid var(--warn)", borderRadius:2}}/>
            <span className="mono" style={{fontSize:10, color:"var(--text-3)", letterSpacing:"0.14em"}}>SIBLING (diversity question)</span>
          </span>
          <span style={{display:"inline-flex", alignItems:"center", gap:5}}>
            <span style={{width:14, height:1, borderTop:"1px dashed var(--warn)"}}/>
            <span className="mono" style={{fontSize:10, color:"var(--text-3)", letterSpacing:"0.14em"}}>FORK</span>
          </span>
        </div>
      </div>

      {/* Variant table */}
      <div style={{borderTop:"1px solid var(--border-soft)"}}>
        <table style={{width:"100%", borderCollapse:"collapse"}}>
          <thead>
            <tr>
              {["Variant","Content hash","Parent","Born","Trades","Realized PnL","Sharpe"].map((h, i) => (
                <th key={i} className="ulabel" style={{
                  padding:"9px 14px", borderBottom:"1px solid var(--border-soft)",
                  textAlign: i >= 4 ? "right" : "left", fontSize:9.5, fontWeight:600,
                }}>{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {VARIANTS.map((v, i) => (
              <tr key={v.id} style={{borderBottom: i < VARIANTS.length-1 ? "1px solid var(--border-soft)" : "none"}}>
                <td style={{padding:"8px 14px"}}>
                  <span className="mono" style={{fontSize:11.5, color: v.current ? "var(--gold)" : "var(--text)"}}>{v.id}</span>
                  {v.current && <span className="mono" style={{fontSize:9.5, color:"var(--gold)", letterSpacing:"0.18em", marginLeft:8}}>HEAD</span>}
                </td>
                <td style={{padding:"8px 14px"}}>
                  <span className="mono" style={{fontSize:11, color:"var(--text-2)"}}>blake3:{v.hash}</span>
                </td>
                <td style={{padding:"8px 14px"}}>
                  <span className="mono" style={{fontSize:11, color: v.parent ? "var(--text-2)" : "var(--text-4)"}}>{v.parent || "—"}</span>
                </td>
                <td style={{padding:"8px 14px"}}>
                  <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>{v.born}</span>
                </td>
                <td style={{padding:"8px 14px", textAlign:"right"}}>
                  <span className="mono" style={{fontSize:11.5, color:"var(--text-2)"}}>{v.trades}</span>
                </td>
                <td style={{padding:"8px 14px", textAlign:"right"}}>
                  <span className="mono" style={{fontSize:11.5, color:"var(--gold)"}}>{v.pnl}</span>
                </td>
                <td style={{padding:"8px 14px", textAlign:"right"}}>
                  <span className="mono" style={{fontSize:12, color: v.current ? "var(--gold)" : "var(--text)"}}>{v.sharpe}</span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Card>
  );
};

// === Attestation verdicts card ===
const AttestationVerdictsCard = () => (
  <Card
    title="Attestation verdicts"
    sub={`5 verdicts · 4 endorse · 1 question · 0 reject`}
    right={<Btn variant="ghost" dense icon="ext">All on-chain</Btn>}
  >
    <div>
      {LINEAGE_ATTESTATIONS.map((a, i) => {
        const tone = a.verdict === "ENDORSE" ? "var(--gold)"
                   : a.verdict === "QUESTION" ? "var(--warn)" : "var(--danger)";
        return (
          <div key={i} style={{
            padding:"10px 14px",
            borderBottom: i < LINEAGE_ATTESTATIONS.length-1 ? "1px solid var(--border-soft)" : "none",
          }}>
            <div style={{display:"flex", alignItems:"center", gap:10}}>
              <span style={{
                minWidth:74, display:"inline-flex", alignItems:"center", gap:5,
                padding:"3px 7px", border:`1px solid ${tone}`, borderRadius:3,
              }}>
                <span style={{width:5, height:5, borderRadius:"50%", background:tone}}/>
                <span className="mono" style={{fontSize:9.5, color:tone, letterSpacing:"0.14em", fontWeight:600}}>{a.verdict}</span>
              </span>
              <span className="mono" style={{fontSize:11, color:"var(--text-2)"}}>{a.attester}</span>
              <span className="mono" style={{fontSize:10.5, color:"var(--gold)"}}>NFT {a.token}</span>
              <span style={{marginLeft:"auto"}} className="mono"><span style={{fontSize:10.5, color:"var(--text-3)"}}>→ </span>
                <span style={{fontSize:11, color:"var(--text)"}}>{a.target}</span>
              </span>
            </div>
            <div style={{display:"flex", alignItems:"center", gap:10, marginTop:5}}>
              <div style={{flex:1, fontSize:11.5, color:"var(--text-2)"}}>{a.rationale}</div>
              <TxChip hash={a.tx}/>
              <span className="mono" style={{fontSize:10.5, color:"var(--text-3)", minWidth:48, textAlign:"right"}}>{a.t}</span>
            </div>
          </div>
        );
      })}
    </div>
  </Card>
);

// === Merkle proof card ===
const MerkleProofCard = () => {
  // Render a small 3-level merkle tree: 4 leaves -> 2 mid -> 1 root
  // Leaves represent the per-variant tuples: parent_hash, child_hash, days_alive, trades, pnl
  return (
    <Card
      title="Merkle anchor"
      sub={`receipt_kind=Snapshot · ${LINEAGE.merkleTx}`}
      right={<Btn variant="ghost" dense icon="ext">Verify on-chain</Btn>}
    >
      <div style={{padding:"14px 16px"}}>
        <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em", marginBottom:6}}>MERKLE ROOT</div>
        <div className="mono" style={{
          fontSize:12, color:"var(--gold)", padding:"7px 9px",
          background:"var(--gold-bg)", border:"1px solid var(--gold-soft)", borderRadius:4,
          wordBreak:"break-all",
        }}>{LINEAGE.merkleRoot}</div>

        {/* tree visual */}
        <svg width="100%" height="100" viewBox="0 0 320 100" style={{marginTop:14, display:"block"}}>
          {/* connectors */}
          <line x1="80"  y1="14" x2="40"  y2="46" stroke="var(--border-strong)" strokeWidth="1"/>
          <line x1="80"  y1="14" x2="120" y2="46" stroke="var(--border-strong)" strokeWidth="1"/>
          <line x1="240" y1="14" x2="200" y2="46" stroke="var(--border-strong)" strokeWidth="1"/>
          <line x1="240" y1="14" x2="280" y2="46" stroke="var(--border-strong)" strokeWidth="1"/>
          <line x1="160" y1="-10" x2="80"  y2="14" stroke="var(--gold-soft)" strokeWidth="1.5"/>
          <line x1="160" y1="-10" x2="240" y2="14" stroke="var(--gold-soft)" strokeWidth="1.5"/>
          {/* mid hashes */}
          <rect x="56"  y="2"  width="48" height="22" rx="3" fill="var(--surface-elev)" stroke="var(--border-strong)"/>
          <rect x="216" y="2"  width="48" height="22" rx="3" fill="var(--surface-elev)" stroke="var(--border-strong)"/>
          <text x="80"  y="17" textAnchor="middle" fontFamily="Geist Mono" fontSize="9.5" fill="#9CA3AF">h(L,R)</text>
          <text x="240" y="17" textAnchor="middle" fontFamily="Geist Mono" fontSize="9.5" fill="#9CA3AF">h(L,R)</text>
          {/* leaves */}
          {[20,100,180,260].map((x, i) => (
            <g key={i}>
              <rect x={x} y="46" width="40" height="40" rx="3" fill="var(--surface-elev)" stroke="var(--border-strong)"/>
              <text x={x+20} y="60" textAnchor="middle" fontFamily="Geist Mono" fontSize="8.5" fill="#5F6670">leaf {i}</text>
              <text x={x+20} y="73" textAnchor="middle" fontFamily="Geist Mono" fontSize="8.5" fill="#9CA3AF">v{i+1}</text>
              <text x={x+20} y="82" textAnchor="middle" fontFamily="Geist Mono" fontSize="7.5" fill="#5F6670">{["c4d…", "9a8…", "f12…", "b1e…"][i]}</text>
            </g>
          ))}
        </svg>

        <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em", marginTop:14, marginBottom:6}}>LEAF FIELDS (per variant)</div>
        <div className="mono" style={{fontSize:11, color:"var(--text-2)", lineHeight:1.7}}>
          parent_hash <span style={{color:"var(--text-4)"}}>→</span> child_hash <span style={{color:"var(--text-4)"}}>→</span> days_alive <span style={{color:"var(--text-4)"}}>→</span> trades_attributed <span style={{color:"var(--text-4)"}}>→</span> realized_pnl_attributed
        </div>

        <div style={{
          marginTop:14, padding:"8px 10px",
          border:"1px solid var(--border-soft)", borderRadius:4,
          display:"flex", alignItems:"center", gap:8,
        }}>
          <Icon name="check" size={12} color="var(--gold)"/>
          <span className="mono" style={{fontSize:11, color:"var(--text-2)"}}>
            Root matches local recomputation · <span style={{color:"var(--gold)"}}>byte-identical</span>
          </span>
        </div>
      </div>
    </Card>
  );
};

// === Right rail: Manifest card ===
const ManifestCard = () => {
  const rows = [
    ["lineage_id",       LINEAGE.name,           "mono"],
    ["nft_token_id",     LINEAGE.token,          "gold"],
    ["agentURI",         `ipfs://${LINEAGE.manifestCid}`, "link"],
    ["initial_bundle",   "blake3:7f2b1ad…91c4",  "mono"],
    ["parent_lineage",   LINEAGE.parent || "— (seed)", "muted"],
    ["born_at",          LINEAGE.born,           "mono"],
    ["operator_sig",     LINEAGE.operatorSig,    "mono"],
    ["session_id",       LINEAGE.sessionId,      "mono"],
  ];
  return (
    <Card
      title="Manifest"
      sub="uploaded to IPFS · CID is agentURI"
      right={<Btn variant="ghost" dense icon="copy"/>}
    >
      <div style={{padding:"10px 14px"}}>
        {rows.map(([k, v, t], i) => (
          <div key={k} style={{
            display:"grid", gridTemplateColumns:"120px 1fr",
            padding:"7px 0", gap:10,
            borderBottom: i < rows.length-1 ? "1px solid var(--border-soft)" : "none",
            alignItems:"center",
          }}>
            <span className="ulabel" style={{fontSize:9.5, letterSpacing:"0.14em"}}>{k}</span>
            <span className="mono" style={{
              fontSize:11, wordBreak:"break-all",
              color: t === "gold" ? "var(--gold)"
                  : t === "muted" ? "var(--text-3)"
                  : t === "link" ? "var(--info)"
                  : "var(--text)",
              textDecoration: t === "link" ? "underline dotted" : "none",
              textUnderlineOffset:2,
            }}>{v}</span>
          </div>
        ))}
      </div>
    </Card>
  );
};

// === Right rail: On-chain receipts list ===
const ReceiptsCard = () => (
  <Card
    title="On-chain receipts"
    sub={`${RECEIPTS.filter(r=>r.tx).length} of ${RECEIPTS.length} on-chain · total gas 0.0058 ETH`}
    style={{flex:1, minHeight:0}}
    bodyStyle={{overflowY:"auto"}}
  >
    <div>
      {RECEIPTS.map((r, i) => (
        <div key={i} style={{
          padding:"10px 14px",
          borderBottom: i < RECEIPTS.length-1 ? "1px solid var(--border-soft)" : "none",
        }}>
          <div style={{display:"flex", alignItems:"center", gap:8}}>
            <span className="ulabel" style={{
              fontSize:9.5, letterSpacing:"0.16em",
              color: r.kind === "Merkle" ? "var(--info)"
                   : r.kind === "Mint" ? "var(--gold)"
                   : r.kind === "Validation" ? "var(--text)"
                   : "var(--text-2)",
            }}>{r.kind}</span>
            <span style={{marginLeft:"auto"}} className="mono">
              <span style={{fontSize:10.5, color:"var(--text-3)"}}>{r.t}</span>
            </span>
          </div>
          <div style={{fontSize:11.5, color:"var(--text)", marginTop:4}}>{r.label}</div>
          <div style={{display:"flex", alignItems:"center", gap:8, marginTop:6}}>
            {r.tx ? <TxChip hash={r.tx}/> : (
              <span className="mono" style={{fontSize:10.5, color:"var(--text-4)"}}>off-chain · referenced in manifest</span>
            )}
            {r.gas && <span className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginLeft:"auto"}}>{r.gas}</span>}
          </div>
        </div>
      ))}
    </div>
  </Card>
);

window.LineageDetail = LineageDetail;
