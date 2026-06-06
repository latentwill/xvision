// Frame — /marketplace/creator/<handle-or-address> · public creator profile
//
// Per §5.1 of the marketplace design direction: fully derived from chain.
// Wallet IS the account. No off-chain signup. Computed:
//   - All strategies minted by the address (IdentityRegistry events)
//   - Lineage tree across all their work (parent/child + clone edges)
//   - Lifetime earnings (sum of LicenseToken purchases minus platform fee)
//   - Attestations issued + received (ValidationRegistry, ReputationRegistry)
//   - Cloned-from / cloned-by edges
//
// This is where "OG creator" virality lives.

const CREATOR = {
  handle: "@ed",
  address: "0xa83e…f12d4",
  fullAddress: "0xa83e7c2efabb91d4eea7c2efbb91d4eef12d4",
  seed: "ed-0xa83e-creator",
  joinedAt: "2025-08-12",
  joinedRel: "9mo ago",
  ensName: "ed.xvn",            // optional handle source
  reputation: 4.8,
  // counter flex
  strategiesCount: 3,
  lifetimeEarned: "$4,820",
  totalBuyersH: 469,
  totalBuyersA: 27,
  clonesSpawned: 11,
  attestationsIssued: 14,
  // notable flag
  notableTag: "agent #0 contributor",
  // strategies
  strategies: [
    { id:"btc-momentum-v3", ver:"v3.0", seed:"btc-momentum-7a91-v3",
      assets:["BTC"], ret30:"+47.2%", buyersH:247, buyersA:14, price:"49 USDC",
      cloned:8, verified:true, x402:true, status:"live" },
    { id:"btc-grid-v2", ver:"v2.3", seed:"btc-grid-6f5b",
      assets:["BTC"], ret30:"+31.4%", buyersH:134, buyersA:9, price:"69 USDC",
      cloned:3, verified:true, x402:false, status:"live" },
    { id:"eth-mr-v2", ver:"v2.0", seed:"eth-mr-3b22-v2",
      assets:["ETH"], ret30:"+12.8%", buyersH:88, buyersA:3, price:"39 USDC",
      cloned:0, verified:true, x402:false, status:"live" },
  ],
  attestationsActivity: [
    { kind:"received", verdict:"ENDORSE",  attester:"regime-verifier", on:"btc-momentum-v3", t:"1h ago" },
    { kind:"received", verdict:"ENDORSE",  attester:"diversity-check", on:"btc-momentum-v3", t:"1h ago" },
    { kind:"issued",   verdict:"ENDORSE",  attester:"@ed",             on:"sol-strategist-pro", t:"8h ago" },
    { kind:"received", verdict:"QUESTION", attester:"diversity-check", on:"btc-momentum-v3.1", t:"4h ago" },
    { kind:"issued",   verdict:"ENDORSE",  attester:"@ed",             on:"eth-swing", t:"1d ago" },
  ],
  // lineage forest: each strategy is a tree of variants; clones from others come in
  forestNodes: [
    // btc-momentum tree
    { id:"bm-v1", x:60,  y:50, label:"v1.0", strategy:"btc-momentum", current:false, seed:"btc-momentum-7a91-v1" },
    { id:"bm-v2", x:160, y:50, label:"v2.0", strategy:"btc-momentum", current:false, seed:"btc-momentum-7a91-v2" },
    { id:"bm-v3", x:260, y:50, label:"v3.0", strategy:"btc-momentum", current:true,  seed:"btc-momentum-7a91-v3" },
    // btc-grid tree
    { id:"bg-v1", x:60,  y:140, label:"v1.0", strategy:"btc-grid",    current:false, seed:"btc-grid-6f5b-v1" },
    { id:"bg-v2", x:160, y:140, label:"v2.3", strategy:"btc-grid",    current:true,  seed:"btc-grid-6f5b" },
    // eth-mr tree (forked from someone else)
    { id:"eth-orig", x:-40, y:230, label:"@kaori v1", strategy:"clone-from", external:true, seed:"eth-mr-3b22-orig" },
    { id:"em-v1", x:60,  y:230, label:"v1.0",   strategy:"eth-mr", current:false, seed:"eth-mr-3b22-v1" },
    { id:"em-v2", x:160, y:230, label:"v2.0",   strategy:"eth-mr", current:true,  seed:"eth-mr-3b22-v2" },
    // clones-by-others off btc-momentum-v3
    { id:"cb-1", x:380, y:20,  label:"@solyana", strategy:"clone-by", external:true, seed:"clone-solyana" },
    { id:"cb-2", x:380, y:60,  label:"@quantnext", strategy:"clone-by", external:true, seed:"clone-quantnext" },
    { id:"cb-3", x:380, y:100, label:"+6 more",    strategy:"clone-by", external:true, more:true },
  ],
  forestEdges: [
    // btc-momentum trunk
    ["bm-v1","bm-v2"], ["bm-v2","bm-v3"],
    // btc-grid trunk
    ["bg-v1","bg-v2"],
    // eth-mr cloned in
    ["eth-orig","em-v1","clone"], ["em-v1","em-v2"],
    // clones-by-others
    ["bm-v3","cb-1","clone"], ["bm-v3","cb-2","clone"], ["bm-v3","cb-3","clone"],
  ],
  // earnings sparkline data (weekly USDC)
  earningsWeekly: [40, 65, 80, 120, 95, 180, 210, 250, 220, 310, 410, 380, 520, 640, 780, 920, 880, 1020, 1180, 1320, 1480, 1620, 1820, 2110, 2380, 2640, 2980, 3340, 3820, 4200, 4520, 4820],
};

const CreatorProfile = () => (
  <TallFrame>
    <SideNav active="marketplace"/>
    <main style={{display:"flex", flexDirection:"column", minWidth:0, overflow:"hidden"}}>
      <TopStatus breadcrumb={[
        { text:"MARKETPLACE" },
        { text:"creator" },
        { text:CREATOR.handle, mono:true },
      ]}/>

      <div style={{flex:1, minHeight:0, overflowY:"auto"}}>

        {/* === HERO === */}
        <div style={{
          padding:"22px 28px 18px 44px", borderBottom:"1px solid var(--border)",
          display:"grid", gridTemplateColumns:"96px 1fr 280px", gap:22,
          alignItems:"center",
        }}>
          {/* Identicon — deterministic from address */}
          <GenArt seed={CREATOR.seed} size={96}
            style={{borderRadius:8, border:"1px solid var(--border)"}}/>

          {/* Handle + address + meta */}
          <div style={{minWidth:0}}>
            <div style={{display:"flex", alignItems:"center", gap:10, flexWrap:"wrap"}}>
              <h1 style={{
                margin:0, fontSize:28, fontWeight:600, letterSpacing:"-0.02em", lineHeight:1,
                fontFamily:"'Geist Mono', monospace",
              }}>{CREATOR.handle}</h1>
              <span style={{
                padding:"3px 8px", borderRadius:3,
                border:"1px solid var(--border-strong)", background:"var(--surface-elev)",
              }}>
                <span className="mono" style={{fontSize:11, color:"var(--text-2)"}}>{CREATOR.ensName}</span>
              </span>
              <span style={{
                padding:"3px 8px", borderRadius:3,
                border:"1px solid var(--gold-soft)", background:"var(--gold-bg)",
                display:"inline-flex", alignItems:"center", gap:5,
              }}>
                <Icon name="shield" size={10} color="var(--gold)"/>
                <span className="mono" style={{fontSize:10, color:"var(--gold)", letterSpacing:"0.14em", fontWeight:600}}>
                  {CREATOR.notableTag.toUpperCase()}
                </span>
              </span>
            </div>
            <div style={{display:"flex", alignItems:"center", gap:10, marginTop:8, flexWrap:"wrap"}}>
              <span className="mono" style={{fontSize:12, color:"var(--text-2)"}}>{CREATOR.address}</span>
              <Btn variant="ghost" dense icon="copy" style={{padding:"3px 7px"}}/>
              <Btn variant="ghost" dense icon="ext" style={{padding:"3px 7px"}}>Mantlescan</Btn>
              <span style={{color:"var(--text-4)"}}>·</span>
              <span className="mono" style={{fontSize:11.5, color:"var(--text-3)"}}>joined {CREATOR.joinedAt}</span>
              <span style={{color:"var(--text-4)"}}>·</span>
              <span className="mono" style={{fontSize:11.5, color:"var(--text-3)"}}>
                rep <span style={{color:"var(--gold)"}}>{CREATOR.reputation}</span>/5
              </span>
            </div>
          </div>

          {/* Actions */}
          <div style={{display:"flex", flexDirection:"column", gap:8}}>
            <Btn variant="primary" icon="plus" style={{justifyContent:"center"}}>Follow @ed</Btn>
            <div style={{display:"flex", gap:6}}>
              <Btn variant="ghost" icon="ext" style={{flex:1, justifyContent:"center"}}>Share profile</Btn>
              <Btn variant="ghost" icon="branch" style={{flex:1, justifyContent:"center"}}>Tip</Btn>
            </div>
          </div>
        </div>

        {/* === COUNTER FLEX === */}
        <div style={{
          padding:"0 28px 0 44px", borderBottom:"1px solid var(--border)",
          display:"grid", gridTemplateColumns:"repeat(6, 1fr)",
        }}>
          <CreatorStat label="Strategies"     value={CREATOR.strategiesCount} mono/>
          <CreatorStat label="Lifetime earned" value={CREATOR.lifetimeEarned} tone="gold"/>
          <CreatorStat label="Total buyers"    value={CREATOR.totalBuyersH}
            sub={<span style={{display:"inline-flex", alignItems:"center", gap:3,
              color:"var(--gold)"}}>
                <AgentIcon size={10}/>+{CREATOR.totalBuyersA}
              </span>}/>
          <CreatorStat label="Clones spawned"  value={CREATOR.clonesSpawned} mono
            sub={<span style={{color:"var(--text-3)"}}>upstream of $2.1k</span>}/>
          <CreatorStat label="Attestations"    value={CREATOR.attestationsIssued} mono
            sub={<span style={{color:"var(--text-3)"}}>issued</span>}/>
          <CreatorStat label="Member since"    value={CREATOR.joinedRel} mono/>
        </div>

        {/* === STRATEGIES + EARNINGS row === */}
        <div style={{
          padding:"18px 28px 0",
          display:"grid", gridTemplateColumns:"1fr 380px", gap:24,
        }}>
          {/* Strategies grid */}
          <Card title="Strategies"
            sub={`${CREATOR.strategiesCount} on chain · sorted by buyers`}
            right={
              <div style={{display:"flex", gap:6}}>
                <Btn variant="chip" dense>All</Btn>
                <Btn variant="ghost" dense>Live</Btn>
                <Btn variant="ghost" dense>Archived</Btn>
              </div>
            }
          >
            <div style={{
              padding:"14px 16px",
              display:"grid", gridTemplateColumns:"1fr 1fr 1fr", gap:12,
            }}>
              {CREATOR.strategies.map((s) => (
                <CreatorStrategyCard key={s.id} s={s}/>
              ))}
            </div>
          </Card>

          {/* Earnings chart */}
          <Card title="Earnings · weekly"
            sub="USDC paid to wallet · 5% platform fee deducted"
          >
            <div style={{padding:"14px 16px 6px"}}>
              <EarningsChart data={CREATOR.earningsWeekly}/>
              <div style={{
                marginTop:10, display:"flex", justifyContent:"space-between",
                fontFamily:"'Geist Mono', monospace", fontSize:10.5,
                color:"var(--text-3)",
              }}>
                <span>32 weeks ago</span>
                <span>today</span>
              </div>
              <div style={{
                marginTop:10, padding:"10px 12px",
                border:"1px solid var(--border-soft)", borderRadius:4,
                display:"flex", alignItems:"center", gap:10,
              }}>
                <Icon name="chart" size={13} color="var(--gold)"/>
                <span className="mono" style={{fontSize:11.5, color:"var(--text-2)"}}>
                  <span style={{color:"var(--gold)"}}>+$420</span> last 7d ·
                  <span style={{color:"var(--gold)"}}> +$1,180</span> last 30d
                </span>
              </div>
            </div>
          </Card>
        </div>

        {/* === LINEAGE FOREST === */}
        <div style={{padding:"18px 28px 0"}}>
          <Card
            title="Lineage forest"
            sub="every strategy + variants + clone edges, on chain · 5 lineages tracked"
            right={
              <div style={{display:"flex", gap:6, alignItems:"center"}}>
                <span className="mono" style={{fontSize:10, color:"var(--text-3)"}}>
                  legend:
                </span>
                <LegendDot color="var(--gold)" label="HEAD"/>
                <LegendDot color="var(--text-2)" label="HISTORY"/>
                <LegendDot color="var(--info)" stroke="dashed" label="CLONE"/>
                <Btn variant="ghost" dense icon="ext">Expand</Btn>
              </div>
            }
          >
            <LineageForest/>
          </Card>
        </div>

        {/* === ATTESTATIONS row === */}
        <div style={{
          padding:"18px 28px 28px",
          display:"grid", gridTemplateColumns:"1fr 1fr", gap:18,
        }}>
          <AttestationsActivityCard/>
          <ClonesByCard/>
        </div>

      </div>
    </main>
  </TallFrame>
);

// === Stat tile ===
const CreatorStat = ({ label, value, sub, tone = "text", mono = false }) => (
  <div style={{
    padding:"16px 14px 16px 0",
    borderRight:"1px solid var(--border)",
  }}>
    <div className="ulabel" style={{fontSize:9, letterSpacing:"0.2em", marginBottom:6}}>
      {label.toUpperCase()}
    </div>
    <div style={{
      fontSize:24, fontWeight:600, letterSpacing:"-0.01em", lineHeight:1,
      color: tone === "gold" ? "var(--gold)" : "var(--text)",
      fontFamily: mono ? "'Geist Mono', monospace" : "'Geist', sans-serif",
    }}>{value}</div>
    {sub && (
      <div className="mono" style={{fontSize:10.5, marginTop:4, color:"var(--text-3)"}}>
        {sub}
      </div>
    )}
  </div>
);

// === Strategy card (compact for creator profile) ===
const CreatorStrategyCard = ({ s }) => (
  <div style={{
    border:"1px solid var(--border)", borderRadius:5,
    overflow:"hidden", background:"#070707", cursor:"pointer",
  }}>
    <div style={{padding:"10px 12px", display:"flex", alignItems:"center", gap:10}}>
      <GenArt seed={s.seed} size={46}/>
      <div style={{flex:1, minWidth:0}}>
        <div style={{display:"flex", alignItems:"center", gap:6, flexWrap:"wrap"}}>
          <span className="mono" style={{fontSize:12, color:"var(--text)", fontWeight:600}}>{s.id}</span>
          <span className="mono" style={{fontSize:10, color:"var(--text-3)"}}>{s.ver}</span>
        </div>
        <div style={{display:"flex", gap:5, marginTop:5}}>
          {s.assets.map((a) => <AssetPill key={a} a={a}/>)}
          {s.verified && <VerifiedBadge/>}
          {s.x402 && <X402Badge/>}
        </div>
      </div>
    </div>
    <div style={{
      padding:"10px 12px", borderTop:"1px solid var(--border-soft)",
      display:"grid", gridTemplateColumns:"1fr 1fr 1fr", gap:8,
    }}>
      <div>
        <div className="ulabel" style={{fontSize:8.5, letterSpacing:"0.16em"}}>30D</div>
        <div className="mono" style={{fontSize:13, color:"var(--gold)", fontWeight:600, marginTop:2}}>{s.ret30}</div>
      </div>
      <div>
        <div className="ulabel" style={{fontSize:8.5, letterSpacing:"0.16em"}}>BUYERS</div>
        <div style={{display:"flex", alignItems:"center", gap:5, marginTop:2}}>
          <span className="mono" style={{fontSize:12, color:"var(--text)"}}>{s.buyersH}</span>
          <span style={{display:"inline-flex", alignItems:"center", gap:2,
            fontFamily:"'Geist Mono', monospace", fontSize:10.5, color:"var(--gold)"}}>
            <AgentIcon size={8}/>{s.buyersA}
          </span>
        </div>
      </div>
      <div>
        <div className="ulabel" style={{fontSize:8.5, letterSpacing:"0.16em"}}>CLONES</div>
        <div className="mono" style={{fontSize:12, color: s.cloned > 0 ? "var(--text)" : "var(--text-3)", marginTop:2}}>
          {s.cloned > 0 ? `${s.cloned}` : "—"}
        </div>
      </div>
    </div>
  </div>
);

// === Earnings chart ===
const EarningsChart = ({ data }) => {
  const w = 320, h = 110, padL = 0, padR = 0, padT = 4, padB = 4;
  const innerW = w - padL - padR;
  const innerH = h - padT - padB;
  const min = 0, max = Math.max(...data);
  const xs = data.map((_, i) => padL + (i / (data.length - 1)) * innerW);
  const ys = data.map((v) => padT + innerH - ((v - min) / (max - min)) * innerH);
  const d = xs.map((x, i) => `${i === 0 ? "M" : "L"} ${x.toFixed(1)} ${ys[i].toFixed(1)}`).join(" ");
  const dFill = d + ` L ${xs[xs.length-1].toFixed(1)} ${(padT+innerH).toFixed(1)} L ${xs[0].toFixed(1)} ${(padT+innerH).toFixed(1)} Z`;
  return (
    <svg width="100%" viewBox={`0 0 ${w} ${h}`} style={{display:"block"}}>
      <defs>
        <linearGradient id="earn-fill" x1="0" x2="0" y1="0" y2="1">
          <stop offset="0%" stopColor="#00E676" stopOpacity="0.30"/>
          <stop offset="100%" stopColor="#00E676" stopOpacity="0"/>
        </linearGradient>
      </defs>
      <path d={dFill} fill="url(#earn-fill)"/>
      <path d={d} fill="none" stroke="var(--gold)" strokeWidth="1.8" strokeLinejoin="round"/>
    </svg>
  );
};

// === Lineage forest ===
const LineageForest = () => {
  const nodes = CREATOR.forestNodes;
  const edges = CREATOR.forestEdges;
  const byId = Object.fromEntries(nodes.map((n) => [n.id, n]));
  // Coordinate translation: original space is ~440x280, we render in ~460x320
  // with small padding.
  const offsetX = 100, offsetY = 30;
  return (
    <div style={{padding:"18px 18px 22px", position:"relative", overflowX:"auto"}}>
      <svg width="100%" height="300" viewBox="-100 0 580 320"
        style={{display:"block"}}>
        {/* edges */}
        {edges.map((e, i) => {
          const [a, b, kind] = e;
          const na = byId[a], nb = byId[b];
          if (!na || !nb) return null;
          const isClone = kind === "clone";
          const x1 = na.x + offsetX, y1 = na.y + offsetY;
          const x2 = nb.x + offsetX, y2 = nb.y + offsetY;
          // curved if y differs
          const dx = (x2 - x1) * 0.5;
          const path = y1 === y2
            ? `M ${x1+22} ${y1} L ${x2-22} ${y2}`
            : `M ${x1+22} ${y1} C ${x1+dx+22} ${y1}, ${x2-dx-22} ${y2}, ${x2-22} ${y2}`;
          return (
            <path key={i} d={path} fill="none"
              stroke={isClone ? "var(--info)" : "var(--border-strong)"}
              strokeWidth={isClone ? 1.2 : 1.4}
              strokeDasharray={isClone ? "3 3" : "none"}
              opacity={isClone ? 0.7 : 0.9}/>
          );
        })}
        {/* lineage row labels */}
        <text x="-50" y={50 + offsetY + 4}  fontFamily="Geist Mono" fontSize="9" fill="#5F6670" letterSpacing="0.18em">
          BTC-MOMENTUM
        </text>
        <text x="-50" y={140 + offsetY + 4} fontFamily="Geist Mono" fontSize="9" fill="#5F6670" letterSpacing="0.18em">
          BTC-GRID
        </text>
        <text x="-50" y={230 + offsetY + 4} fontFamily="Geist Mono" fontSize="9" fill="#5F6670" letterSpacing="0.18em">
          ETH-MR
        </text>
      </svg>
      {/* node tiles — absolutely positioned over the SVG */}
      {nodes.map((n) => {
        const isHead = n.current;
        const isClone = n.strategy === "clone-by" || n.strategy === "clone-from";
        // map SVG coords to relative px positions inside the container.
        // Container is ~580 wide viewBox, rendered at 100% width. Use percentages.
        const leftPct = ((n.x + offsetX + 100) / 580) * 100;
        const topPx   = n.y + offsetY;
        return (
          <div key={n.id} style={{
            position:"absolute",
            left:`${leftPct}%`, top: 18 + topPx,
            transform:"translate(-50%, -50%)",
            display:"flex", flexDirection:"column", alignItems:"center", gap:4,
          }}>
            {n.more ? (
              <div style={{
                width:36, height:36, borderRadius:4,
                border:"1px dashed var(--info)", background:"transparent",
                display:"flex", alignItems:"center", justifyContent:"center",
                fontFamily:"'Geist Mono', monospace", fontSize:10.5, color:"var(--info)",
              }}>+6</div>
            ) : (
              <GenArt seed={n.seed || n.id} size={isClone ? 32 : 38}
                style={{
                  border: isHead ? "2px solid var(--gold)" :
                          isClone ? "1px dashed var(--info)" :
                          "1px solid var(--border)",
                  opacity: isClone ? 0.8 : 1,
                }}/>
            )}
            <span className="mono" style={{
              fontSize:9.5,
              color: isHead ? "var(--gold)" :
                     isClone ? "var(--info)" : "var(--text-2)",
              whiteSpace:"nowrap",
            }}>{n.label}</span>
          </div>
        );
      })}
    </div>
  );
};

const LegendDot = ({ color, stroke = "solid", label }) => (
  <span style={{display:"inline-flex", alignItems:"center", gap:4}}>
    <span style={{
      width:8, height:8, borderRadius:2,
      background: stroke === "solid" ? color : "transparent",
      border: stroke === "dashed" ? `1px dashed ${color}` : `1px solid ${color}`,
    }}/>
    <span className="mono" style={{fontSize:9.5, color:"var(--text-3)", letterSpacing:"0.14em"}}>{label}</span>
  </span>
);

// === Attestations activity ===
const AttestationsActivityCard = () => (
  <Card
    title="Reputation"
    sub={`${CREATOR.attestationsIssued} issued · 22 received · 1 question · 0 reject`}
    right={
      <div style={{display:"flex", gap:6}}>
        <Btn variant="chip" dense>All</Btn>
        <Btn variant="ghost" dense>Received</Btn>
        <Btn variant="ghost" dense>Issued</Btn>
      </div>
    }
  >
    <div>
      {CREATOR.attestationsActivity.map((a, i, arr) => {
        const tone = a.verdict === "ENDORSE" ? "var(--gold)" :
                     a.verdict === "QUESTION" ? "var(--warn)" : "var(--danger)";
        return (
          <div key={i} style={{
            padding:"10px 16px",
            borderBottom: i < arr.length-1 ? "1px solid var(--border-soft)" : "none",
            display:"flex", alignItems:"center", gap:10,
          }}>
            <span style={{
              minWidth:80, display:"inline-flex", alignItems:"center", gap:5,
              padding:"3px 7px", border:`1px solid ${tone}`, borderRadius:3,
            }}>
              <span style={{width:5, height:5, borderRadius:"50%", background:tone}}/>
              <span className="mono" style={{fontSize:9.5, color:tone, letterSpacing:"0.14em", fontWeight:600}}>{a.verdict}</span>
            </span>
            <span className="ulabel" style={{
              fontSize:9, letterSpacing:"0.18em",
              color: a.kind === "issued" ? "var(--info)" : "var(--text-3)",
            }}>{a.kind.toUpperCase()}</span>
            <span className="mono" style={{fontSize:11.5, color:"var(--text-2)", flex:1, minWidth:0}}>
              {a.kind === "issued" ? `→ ${a.on}` : `${a.attester} → ${a.on}`}
            </span>
            <span style={{marginLeft:"auto"}} className="mono">
              <span style={{fontSize:10.5, color:"var(--text-3)"}}>{a.t}</span>
            </span>
          </div>
        );
      })}
    </div>
  </Card>
);

// === Clones-by-others ===
const ClonesByCard = () => (
  <Card
    title="Cloned by · downstream"
    sub={`${CREATOR.clonesSpawned} clones of @ed's work · upstream of $2.1k earnings`}
    right={<Btn variant="ghost" dense icon="ext">Tree</Btn>}
  >
    <div>
      {[
        { handle:"@solyana",   src:"btc-momentum-v3", made:"sol-momentum-v1", earned:"$680",  t:"2d ago" },
        { handle:"@quantnext", src:"btc-momentum-v3", made:"multi-asset-rotation", earned:"$420", t:"5d ago" },
        { handle:"@dca-anon",  src:"btc-grid-v2",     made:"btc-dca-conservative", earned:"$190", t:"9d ago" },
        { handle:"@degenray",  src:"btc-momentum-v3", made:"meme-radar", earned:"$520", t:"12d ago" },
        { handle:"+7 more",    src:"…",                made:"—",         earned:"$310", t:"30d", more:true },
      ].map((c, i, arr) => (
        <div key={i} style={{
          padding:"10px 16px",
          borderBottom: i < arr.length-1 ? "1px solid var(--border-soft)" : "none",
          display:"flex", alignItems:"center", gap:10,
        }}>
          <span style={{
            width:24, height:24, borderRadius:"50%",
            background: c.more ? "transparent" : "var(--surface-elev)",
            border:`1px ${c.more ? "dashed" : "solid"} var(--border-strong)`,
            display:"flex", alignItems:"center", justifyContent:"center",
            fontFamily:"'Geist Mono', monospace", fontSize:9.5, color:"var(--text-3)", flexShrink:0,
          }}>{c.more ? "…" : c.handle.slice(1, 2).toUpperCase()}</span>
          <div style={{flex:1, minWidth:0}}>
            <div className="mono" style={{fontSize:11.5, color:"var(--text)"}}>
              <span style={{color:"var(--text)"}}>{c.handle}</span>
              {!c.more && (
                <>
                  <span style={{color:"var(--text-4)", margin:"0 6px"}}>cloned</span>
                  <span style={{color:"var(--text-2)"}}>{c.src}</span>
                  <span style={{color:"var(--text-4)", margin:"0 6px"}}>→</span>
                  <span style={{color:"var(--text-2)"}}>{c.made}</span>
                </>
              )}
            </div>
          </div>
          <span className="mono" style={{fontSize:11.5, color:"var(--gold)", minWidth:60, textAlign:"right"}}>{c.earned}</span>
          <span className="mono" style={{fontSize:10.5, color:"var(--text-3)", minWidth:54, textAlign:"right"}}>{c.t}</span>
        </div>
      ))}
    </div>
  </Card>
);

window.CreatorProfile = CreatorProfile;
