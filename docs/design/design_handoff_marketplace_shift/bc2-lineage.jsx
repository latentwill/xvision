// Frame — /marketplace/lineage/btc-momentum-v3 · public strategy identity page
//
// Above the fold: hero gen-art, big 30d return, buyer counts (humans+agents),
// $X paid to creator, Buy + Clone + Share, ingredient check.
// Below the fold: equity curve, what you get / don't get, lineage mini-tree,
// recent buyers, creator's other strategies.
// At the bottom: "▸ View on-chain receipts" drawer — collapsed by default,
// expanded in the third frame to show auditor view is still there.

const STRATEGY = {
  id: "btc-momentum-v3",
  lineage: "btc-momentum",
  lineageBase: "btc-momentum-7a91",
  ver: "v3.0",
  creator: "@ed",
  creatorAddr: "0xa83e…f12d4",
  promise: "BTC momentum with Claude regime detection. Holds 1–3 days, 2% risk cap.",
  ret30: "+47.2%",
  sharpe: "+1.31",
  winRate: "62%",
  maxDD: "-8.4%",
  avgDur: "1.8d",
  buyersH: 247,
  buyersA: 14,
  paidToCreator: "$1,240",
  feePct: 5,
  price: "49 USDC",
  verified: true,
  x402: true,
  ingredients: [
    { name:"Claude Haiku 4.5", kind:"model",  installed:true },
    { name:"Birdeye MCP",      kind:"mcp",    installed:false },
    { name:"SOL Strategist skill", kind:"skill", installed:false },
    { name:"Mantlescan MCP",   kind:"mcp",    installed:true },
  ],
  variants: [
    { id:"v1.0", parent:null,   seed:"btc-momentum-7a91-v1", sharpe:"+0.88", current:false },
    { id:"v1.1", parent:"v1.0", seed:"btc-momentum-7a91-v11", sharpe:"+1.04", current:false },
    { id:"v2.0", parent:"v1.1", seed:"btc-momentum-7a91-v2",  sharpe:"+1.18", current:false },
    { id:"v2.1", parent:"v2.0", seed:"btc-momentum-7a91-v21", sharpe:"+1.22", current:false },
    { id:"v3.0", parent:"v2.1", seed:"btc-momentum-7a91-v3",  sharpe:"+1.31", current:true  },
  ],
  recentBuyers: [
    { addr:"0x7c2e…aa07", outcome:"+12.4% · 6d",   t:"3m ago",   kind:"human" },
    { addr:"agent #14",   outcome:"running · 2 trades", t:"22m ago", kind:"agent" },
    { addr:"0x4f8a…dc11", outcome:"+3.1% · 1d",    t:"1h ago",   kind:"human" },
    { addr:"0x91bc…aa72", outcome:"+8.8% · 4d",    t:"4h ago",   kind:"human" },
    { addr:"agent #7",    outcome:"+1.4% · 12h",   t:"6h ago",   kind:"agent" },
    { addr:"0xc0a4…f3b2", outcome:"-0.6% · 1d",    t:"9h ago",   kind:"human" },
  ],
  creatorOther: [
    { id:"btc-grid-v2",   seed:"btc-grid-6f5b", ret30:"+31.4%", buyersH:134, buyersA:9 },
    { id:"eth-mr-v2",     seed:"eth-mr-3b22-v2", ret30:"+12.8%", buyersH:88, buyersA:3 },
  ],
};

// Custom frame for tall pages — fills 100% of artboard height
const TallFrame = ({ children }) => (
  <div style={{
    background:"#000", width:"100%", height:"100%", overflow:"hidden",
    display:"grid", gridTemplateColumns:"200px 1fr", position:"relative",
  }}>{children}</div>
);

// Generic identity page; takes an `expanded` flag to control the receipts drawer.
const LineageIdentity = ({ drawerOpen = false }) => (
  <TallFrame>
    <SideNav active="marketplace"/>
    <main style={{display:"flex", flexDirection:"column", minWidth:0, overflow:"hidden"}}>
      <TopStatus breadcrumb={[
        { text:"MARKETPLACE" },
        { text:"lineage" },
        { text:STRATEGY.id, mono:true },
      ]}/>

      {/* Body wraps in vertical scroll */}
      <div style={{flex:1, minHeight:0, overflowY:"auto"}}>

        {/* === HERO — above the fold === */}
        <div style={{
          padding:"22px 28px 20px",
          display:"grid", gridTemplateColumns:"320px 1fr 250px", gap:24,
          borderBottom:"1px solid var(--border)",
        }}>
          {/* hero gen-art */}
          <div style={{position:"relative"}}>
            <GenArt seed={STRATEGY.lineageBase + "-v3"} size={320}
              style={{borderRadius:8, border:"1px solid var(--border)"}}/>
            <div style={{
              position:"absolute", bottom:10, left:10, display:"flex", gap:6,
            }}>
              <span style={{
                padding:"3px 8px", borderRadius:3,
                background:"rgba(0,0,0,0.7)", backdropFilter:"blur(6px)",
              }}>
                <span className="mono" style={{fontSize:10, color:"var(--text)", letterSpacing:"0.14em", fontWeight:600}}>
                  NFT #0043
                </span>
              </span>
            </div>
          </div>

          {/* name + metrics + buyers */}
          <div style={{minWidth:0, display:"flex", flexDirection:"column", gap:14}}>
            <div>
              <div style={{display:"flex", alignItems:"center", gap:10, flexWrap:"wrap"}}>
                <h1 style={{
                  margin:0, fontSize:30, fontWeight:600, letterSpacing:"-0.025em", lineHeight:1,
                  fontFamily:"'Geist Mono', monospace",
                }}>{STRATEGY.id}</h1>
                <span style={{
                  padding:"3px 8px", borderRadius:3,
                  border:"1px solid var(--border-strong)", background:"var(--surface-elev)",
                }}>
                  <span className="mono" style={{fontSize:11, color:"var(--text-2)"}}>{STRATEGY.ver}</span>
                </span>
                <VerifiedBadge/>
                <X402Badge/>
              </div>
              <div style={{display:"flex", alignItems:"center", gap:10, marginTop:8}}>
                <span className="mono" style={{fontSize:13, color:"var(--text-2)"}}>{STRATEGY.creator}</span>
                <span style={{color:"var(--text-4)"}}>·</span>
                <span className="mono" style={{fontSize:11.5, color:"var(--text-3)"}}>{STRATEGY.creatorAddr}</span>
                <span style={{color:"var(--text-4)"}}>·</span>
                <span className="mono" style={{fontSize:11.5, color:"var(--text-3)"}}>Claude Haiku 4.5</span>
              </div>
              <p style={{
                margin:"12px 0 0", fontSize:14.5, color:"var(--text)", lineHeight:1.45,
                maxWidth:480, fontWeight:400,
              }}>{STRATEGY.promise}</p>
            </div>

            {/* big metric */}
            <div style={{
              display:"grid", gridTemplateColumns:"auto 1fr 1fr 1fr 1fr",
              gap:18, alignItems:"end", paddingTop:6,
            }}>
              <div>
                <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.2em", marginBottom:6}}>30D RETURN</div>
                <div className="mono" style={{
                  fontSize:42, fontWeight:600, color:"var(--gold)", letterSpacing:"-0.03em",
                  lineHeight:1,
                }}>{STRATEGY.ret30}</div>
              </div>
              <MetricCell label="Sharpe"  value={STRATEGY.sharpe}/>
              <MetricCell label="Win rate" value={STRATEGY.winRate}/>
              <MetricCell label="Max DD"   value={STRATEGY.maxDD} tone="warn"/>
              <MetricCell label="Avg dur"  value={STRATEGY.avgDur}/>
            </div>

            {/* Buyer count */}
            <div style={{
              display:"flex", alignItems:"center", gap:14,
              padding:"12px 14px", borderRadius:5,
              border:"1px solid var(--border)", background:"var(--surface-elev)",
            }}>
              <div style={{display:"flex", alignItems:"center"}}>
                {/* small avatar bar */}
                {[..."ABCDE"].map((c, i) => (
                  <div key={i} style={{
                    width:22, height:22, borderRadius:"50%",
                    border:"2px solid #000", background:`hsl(${(i * 73) % 360}deg 50% 40%)`,
                    marginLeft: i === 0 ? 0 : -8, display:"flex", alignItems:"center",
                    justifyContent:"center", color:"#fff", fontSize:9.5, fontWeight:600,
                    fontFamily:"'Geist Mono', monospace",
                  }}>{c}</div>
                ))}
                <div style={{
                  width:22, height:22, borderRadius:"50%",
                  border:"2px solid #000", background:"var(--gold-bg)",
                  marginLeft:-8, display:"flex", alignItems:"center", justifyContent:"center",
                }}>
                  <AgentIcon size={11}/>
                </div>
              </div>
              <div style={{flex:1, minWidth:0}}>
                <div style={{fontSize:13.5, color:"var(--text)"}}>
                  Run by <b style={{color:"var(--text)"}}>{STRATEGY.buyersH} humans</b>
                  <span style={{color:"var(--text-3)", margin:"0 6px"}}>+</span>
                  <b style={{color:"var(--gold)"}}>{STRATEGY.buyersA} agents</b>
                </div>
                <div className="mono" style={{fontSize:11, color:"var(--text-3)", marginTop:3}}>
                  <span style={{color:"var(--gold)"}}>{STRATEGY.paidToCreator}</span> paid to {STRATEGY.creator} · {STRATEGY.feePct}% platform fee
                </div>
              </div>
            </div>
          </div>

          {/* RIGHT: CTAs + ingredient check teaser */}
          <div style={{display:"flex", flexDirection:"column", gap:10}}>
            <div style={{
              padding:"16px 14px", borderRadius:6,
              background:"linear-gradient(180deg, rgba(0,230,118,0.06), rgba(0,230,118,0.02))",
              border:"1px solid var(--gold-soft)",
            }}>
              <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em", marginBottom:6}}>PRICE</div>
              <div className="mono" style={{
                fontSize:24, color:"var(--text)", fontWeight:600, letterSpacing:"-0.01em", lineHeight:1,
              }}>{STRATEGY.price}</div>
              <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginTop:4}}>
                perpetual license · one-time
              </div>
              <button style={{
                marginTop:12, width:"100%",
                padding:"10px 12px", borderRadius:4,
                background:"var(--gold)", color:"#001A0A", border:"none",
                fontFamily:"'Geist', sans-serif", fontSize:13.5, fontWeight:700,
                cursor:"pointer", letterSpacing:"0.01em",
              }}>Buy</button>
            </div>
            <div style={{display:"flex", gap:8}}>
              <Btn variant="ghost" icon="branch" style={{flex:1, justifyContent:"center"}}>Clone to edit</Btn>
              <Btn variant="ghost" icon="ext" style={{flex:1, justifyContent:"center"}}>Share</Btn>
            </div>
          </div>
        </div>

        {/* === INGREDIENT CHECK BANNER === */}
        <div style={{
          padding:"14px 28px",
          display:"flex", alignItems:"center", gap:14,
          borderBottom:"1px solid var(--border)",
          background:"rgba(255,176,32,0.04)",
        }}>
          <div style={{
            width:28, height:28, borderRadius:"50%",
            background:"rgba(255,176,32,0.12)", border:"1px solid var(--warn)",
            display:"flex", alignItems:"center", justifyContent:"center", flexShrink:0,
          }}>
            <Icon name="info" size={14} color="var(--warn)"/>
          </div>
          <div style={{flex:1, minWidth:0}}>
            <div style={{fontSize:13.5, color:"var(--text)"}}>
              <b>Ingredient check · 2 of 4 installed in your XVN.</b> Install the missing two before purchase.
            </div>
            <div style={{display:"flex", gap:8, marginTop:7, flexWrap:"wrap"}}>
              {STRATEGY.ingredients.map((ing) => (
                <span key={ing.name} style={{
                  display:"inline-flex", alignItems:"center", gap:6,
                  padding:"3px 8px", borderRadius:3,
                  border:`1px solid ${ing.installed ? "var(--gold-soft)" : "var(--warn)"}`,
                  background: ing.installed ? "var(--gold-bg)" : "rgba(255,176,32,0.08)",
                }}>
                  {ing.installed
                    ? <Icon name="check" size={10} color="var(--gold)" sw={2}/>
                    : <Icon name="plus" size={10} color="var(--warn)" sw={2}/>}
                  <span className="mono" style={{
                    fontSize:11, color: ing.installed ? "var(--gold)" : "var(--warn)",
                  }}>{ing.name}</span>
                  <span className="mono" style={{fontSize:9, color:"var(--text-4)", letterSpacing:"0.14em"}}>
                    {ing.kind.toUpperCase()}
                  </span>
                </span>
              ))}
            </div>
          </div>
          <Btn variant="chip" icon="plus">Install missing</Btn>
        </div>

        {/* === BELOW THE FOLD === */}
        <div style={{padding:"18px 28px 0", display:"grid", gridTemplateColumns:"1fr 380px", gap:24}}>
          <div style={{display:"flex", flexDirection:"column", gap:18, minWidth:0}}>
            <EquityCurveCard/>
            <WhatYouGetCard/>
            <LineageMiniTree/>
          </div>
          <div style={{display:"flex", flexDirection:"column", gap:18, minWidth:0}}>
            <RecentBuyersCard/>
            <CreatorOtherCard/>
          </div>
        </div>

        {/* === ON-CHAIN RECEIPTS DRAWER === */}
        <ReceiptsDrawer open={drawerOpen}/>

      </div>
    </main>
  </TallFrame>
);

const MetricCell = ({ label, value, tone = "text" }) => (
  <div>
    <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em", marginBottom:4}}>{label.toUpperCase()}</div>
    <div className="mono" style={{
      fontSize:16, fontWeight:600, lineHeight:1,
      color: tone === "warn" ? "var(--warn)" : "var(--text)",
    }}>{value}</div>
  </div>
);

// === Equity curve card ===
const EquityCurveCard = () => {
  // Build a fat equity curve: backtest (faded) + live (gold).
  const rng = bc2Rng(bc2Hash("btc-momentum-v3-equity"));
  const pts = [];
  let v = 100;
  for (let i = 0; i < 90; i++) {
    v += 0.4 + (rng() - 0.45) * 3;
    v = Math.max(80, v);
    pts.push(v);
  }
  const w = 720, h = 180, leftPad = 36, rightPad = 12, topPad = 14, botPad = 22;
  const innerW = w - leftPad - rightPad;
  const innerH = h - topPad - botPad;
  const min = Math.min(...pts), max = Math.max(...pts);
  const xs = pts.map((_, i) => leftPad + (i / (pts.length - 1)) * innerW);
  const ys = pts.map((p) => topPad + innerH - ((p - min) / (max - min)) * innerH);
  const liveStart = 60;
  const dBack = xs.slice(0, liveStart).map((x, i) => `${i === 0 ? "M" : "L"} ${x.toFixed(1)} ${ys[i].toFixed(1)}`).join(" ");
  const dLive = xs.slice(liveStart - 1).map((x, i) => {
    const j = i + liveStart - 1;
    return `${i === 0 ? "M" : "L"} ${x.toFixed(1)} ${ys[j].toFixed(1)}`;
  }).join(" ");
  const dFill = xs.map((x, i) => `${i === 0 ? "M" : "L"} ${x.toFixed(1)} ${ys[i].toFixed(1)}`).join(" ")
              + ` L ${xs[xs.length-1].toFixed(1)} ${(topPad+innerH).toFixed(1)}`
              + ` L ${xs[0].toFixed(1)} ${(topPad+innerH).toFixed(1)} Z`;

  return (
    <Card
      title="Equity curve"
      sub="90d · backtest (faded) + live (solid) · base $1,000"
      right={
        <div style={{display:"flex", gap:6}}>
          <Btn variant="chip" dense>If I bought at mint</Btn>
          <Btn variant="ghost" dense>30d</Btn>
          <Btn variant="ghost" dense>90d</Btn>
        </div>
      }
    >
      <div style={{padding:"14px 16px 6px"}}>
        <svg width="100%" viewBox={`0 0 ${w} ${h}`} style={{display:"block"}}>
          <defs>
            <linearGradient id="eq-fill" x1="0" x2="0" y1="0" y2="1">
              <stop offset="0%" stopColor="#00E676" stopOpacity="0.22"/>
              <stop offset="100%" stopColor="#00E676" stopOpacity="0"/>
            </linearGradient>
          </defs>
          {/* horizontal gridlines */}
          {[0.25, 0.5, 0.75].map((f, i) => (
            <line key={i}
              x1={leftPad} x2={w-rightPad}
              y1={topPad + innerH * f} y2={topPad + innerH * f}
              stroke="var(--border-soft)" strokeDasharray="2 4"/>
          ))}
          {/* y axis labels */}
          {[max, (max+min)/2, min].map((v, i) => (
            <text key={i} x={leftPad - 6} y={topPad + (innerH * i / 2) + 3}
              textAnchor="end" fontFamily="Geist Mono" fontSize="9.5" fill="#5F6670">
              ${v.toFixed(0)}
            </text>
          ))}
          {/* fill area */}
          <path d={dFill} fill="url(#eq-fill)"/>
          {/* backtest dashed faded */}
          <path d={dBack} fill="none" stroke="var(--text-3)" strokeWidth="1.2"
            strokeDasharray="3 3" opacity="0.6"/>
          {/* live solid gold */}
          <path d={dLive} fill="none" stroke="var(--gold)" strokeWidth="1.8"/>
          {/* live start marker */}
          <line x1={xs[liveStart].toFixed(1)} x2={xs[liveStart].toFixed(1)}
            y1={topPad} y2={topPad+innerH}
            stroke="var(--gold-soft)" strokeDasharray="2 3" opacity="0.6"/>
          <text x={xs[liveStart].toFixed(1)} y={topPad + 11}
            fontFamily="Geist Mono" fontSize="9.5" fill="#00E676"
            letterSpacing="0.16em">LIVE</text>
          {/* x labels */}
          {[0, 0.5, 1].map((f, i) => (
            <text key={i}
              x={leftPad + innerW * f} y={h - 6}
              textAnchor={f === 0 ? "start" : f === 1 ? "end" : "middle"}
              fontFamily="Geist Mono" fontSize="9.5" fill="#5F6670">
              {f === 0 ? "90d ago" : f === 1 ? "today" : "45d"}
            </text>
          ))}
        </svg>
      </div>
    </Card>
  );
};

// === What you get / don't get ===
const WhatYouGetCard = () => (
  <div style={{display:"grid", gridTemplateColumns:"1fr 1fr", gap:14}}>
    <Card title="What you get" sub="Tier 2 sealed bundle · decrypts after purchase">
      <ul style={{margin:0, padding:"4px 18px 14px 32px", fontSize:13, color:"var(--text-2)", lineHeight:1.7}}>
        <li>Full prompt + system instructions</li>
        <li>Agent topology &amp; ordering</li>
        <li>Threshold values · stop-loss · take-profit rules</li>
        <li>Required MCP &amp; skill configurations</li>
        <li>Creator-shipped backtest notes</li>
      </ul>
    </Card>
    <Card title="What you don't get" sub="Tier 3 — never bundled">
      <ul style={{margin:0, padding:"4px 18px 14px 32px", fontSize:13, color:"var(--text-3)", lineHeight:1.7}}>
        <li>Creator's proprietary data sources</li>
        <li>Future updates without re-purchase</li>
        <li>Creator's research scratch &amp; journal</li>
        <li>Broker / wallet credentials</li>
      </ul>
    </Card>
  </div>
);

// === Lineage mini-tree ===
const LineageMiniTree = () => (
  <Card
    title="Lineage tree"
    sub={`5 variants · cloned by ${8} others · view full tree`}
    right={<Btn variant="ghost" dense icon="ext">Open tree</Btn>}
  >
    <div style={{
      padding:"18px 22px 20px",
      display:"flex", alignItems:"center", gap:0,
    }}>
      {STRATEGY.variants.map((v, i) => (
        <React.Fragment key={v.id}>
          <div style={{
            display:"flex", flexDirection:"column", alignItems:"center", gap:6,
          }}>
            <GenArt seed={v.seed} size={56}
              style={{border: v.current ? "2px solid var(--gold)" : "1px solid var(--border)"}}/>
            <span className="mono" style={{fontSize:10.5, color: v.current ? "var(--gold)" : "var(--text-2)"}}>{v.id}</span>
            <span className="mono" style={{fontSize:9.5, color:"var(--text-3)"}}>{v.sharpe}</span>
          </div>
          {i < STRATEGY.variants.length - 1 && (
            <div style={{
              flex:1, height:1, background:"var(--border-strong)",
              margin:"0 8px", marginTop:-30, position:"relative", maxWidth:54,
            }}>
              <div style={{
                position:"absolute", right:-3, top:-2, width:6, height:6,
                borderRadius:"50%", background:"var(--border-strong)",
              }}/>
            </div>
          )}
        </React.Fragment>
      ))}
      <div style={{flex:1}}/>
      {/* cloned-from-you teaser */}
      <div style={{
        textAlign:"right", paddingLeft:14, borderLeft:"1px solid var(--border)",
      }}>
        <div className="ulabel" style={{fontSize:9, letterSpacing:"0.18em"}}>CLONES OF YOURS</div>
        <div className="mono" style={{fontSize:22, color:"var(--gold)", fontWeight:600, marginTop:2}}>8</div>
        <div className="mono" style={{fontSize:10, color:"var(--text-3)", marginTop:2}}>
          upstream of <span style={{color:"var(--gold)"}}>$2.1k</span>
        </div>
      </div>
    </div>
  </Card>
);

// === Recent buyers ===
const RecentBuyersCard = () => (
  <Card
    title="Recent buyers + outcomes"
    sub="anonymous · chain-verifiable"
  >
    <div style={{padding:"4px 0"}}>
      {STRATEGY.recentBuyers.map((b, i) => {
        const positive = b.outcome.includes("+");
        const isAgent = b.kind === "agent";
        return (
          <div key={i} style={{
            display:"flex", alignItems:"center", gap:10,
            padding:"9px 16px",
            borderBottom: i < STRATEGY.recentBuyers.length-1 ? "1px solid var(--border-soft)" : "none",
          }}>
            <div style={{
              width:22, height:22, borderRadius: isAgent ? 4 : "50%",
              background: isAgent ? "var(--gold-bg)" : "var(--surface-elev)",
              border:`1px solid ${isAgent ? "var(--gold-soft)" : "var(--border-strong)"}`,
              display:"flex", alignItems:"center", justifyContent:"center",
              fontSize:9.5, fontFamily:"'Geist Mono', monospace", color:"var(--text-3)",
            }}>
              {isAgent ? <AgentIcon size={11}/> : "0x"}
            </div>
            <span className="mono" style={{fontSize:11.5, color:"var(--text-2)"}}>{b.addr}</span>
            <span style={{color:"var(--text-4)", margin:"0 4px"}}>·</span>
            <span className="mono" style={{
              fontSize:11.5, color: b.outcome.includes("running") ? "var(--info)" :
                positive ? "var(--gold)" : "var(--danger)",
            }}>{b.outcome}</span>
            <span style={{marginLeft:"auto"}} className="mono">
              <span style={{fontSize:10.5, color:"var(--text-3)"}}>{b.t}</span>
            </span>
          </div>
        );
      })}
    </div>
  </Card>
);

// === Creator's other strategies ===
const CreatorOtherCard = () => (
  <Card
    title={`More from ${STRATEGY.creator}`}
    sub="3 strategies on chain · 469 total buyers"
    right={<Btn variant="ghost" dense icon="ext">Profile</Btn>}
  >
    <div style={{padding:"4px 0"}}>
      {STRATEGY.creatorOther.map((s, i) => (
        <div key={s.id} style={{
          display:"flex", alignItems:"center", gap:12, padding:"10px 16px",
          borderBottom: i < STRATEGY.creatorOther.length-1 ? "1px solid var(--border-soft)" : "none",
          cursor:"pointer",
        }}>
          <GenArt seed={s.seed} size={36}/>
          <div style={{flex:1, minWidth:0}}>
            <div className="mono" style={{fontSize:12.5, color:"var(--text)", fontWeight:600}}>{s.id}</div>
            <div style={{display:"flex", gap:8, marginTop:3}}>
              <span className="mono" style={{fontSize:11, color:"var(--text-2)"}}>
                {s.buyersH}
              </span>
              <span style={{display:"inline-flex", alignItems:"center", gap:3,
                fontFamily:"'Geist Mono', monospace", fontSize:11, color:"var(--gold)"}}>
                <AgentIcon size={9}/>{s.buyersA}
              </span>
            </div>
          </div>
          <span className="mono" style={{fontSize:14, color:"var(--gold)", fontWeight:600}}>{s.ret30}</span>
        </div>
      ))}
    </div>
  </Card>
);

// === On-chain receipts drawer ===
const ReceiptsDrawer = ({ open = false }) => (
  <div style={{
    marginTop:24, borderTop:"1px solid var(--border)",
    background: open ? "#070707" : "transparent",
  }}>
    <div style={{
      padding:"14px 28px",
      display:"flex", alignItems:"center", gap:10, cursor:"pointer",
      borderBottom: open ? "1px solid var(--border)" : "none",
    }}>
      <Icon name={open ? "chevD" : "chevR"} size={13} color="var(--text-2)"/>
      <span style={{fontSize:13.5, fontWeight:500, color:"var(--text)"}}>
        {open ? "Hide" : "View"} on-chain receipts
      </span>
      <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>
        · NFT, manifest hash, attestations, anchor history, validator activity
      </span>
      <span style={{marginLeft:"auto", display:"flex", alignItems:"center", gap:6}}>
        <span className="ulabel" style={{fontSize:9, letterSpacing:"0.18em", color:"var(--text-3)"}}>
          AUDITOR
        </span>
        <Icon name="shield" size={11} color="var(--text-3)"/>
      </span>
    </div>

    {open && (
      <div style={{padding:"18px 28px 28px", display:"grid", gridTemplateColumns:"1fr 1fr", gap:18}}>
        {/* NFT / manifest */}
        <Card title="Identity NFT &amp; manifest" sub="Mantle mainnet · contract 0xCa55…22Be">
          <div style={{padding:"10px 14px"}}>
            {[
              ["nft_token_id", "#0043", "gold"],
              ["lineage_id", "btc-momentum", "mono"],
              ["agentURI", "ipfs://bafybeib4xj…q2y7l", "link"],
              ["manifest_hash", "blake3:7f2b1ad…91c4", "mono"],
              ["parent_lineage", "— (seed)", "muted"],
              ["born_at", "2026-05-13 04:12Z", "mono"],
              ["operator_sig", "ed25519:7f2b1ad…91c4", "mono"],
            ].map(([k, v, t], i, arr) => (
              <div key={k} style={{
                display:"grid", gridTemplateColumns:"120px 1fr", gap:10,
                padding:"7px 0",
                borderBottom: i < arr.length-1 ? "1px solid var(--border-soft)" : "none",
                alignItems:"center",
              }}>
                <span className="ulabel" style={{fontSize:9.5, letterSpacing:"0.14em"}}>{k}</span>
                <span className="mono" style={{
                  fontSize:11, wordBreak:"break-all",
                  color: t === "gold" ? "var(--gold)" : t === "muted" ? "var(--text-3)" :
                         t === "link" ? "var(--info)" : "var(--text)",
                  textDecoration: t === "link" ? "underline dotted" : "none",
                }}>{v}</span>
              </div>
            ))}
          </div>
        </Card>

        {/* Attestations */}
        <Card title="Attestation verdicts" sub="5 verdicts · 4 endorse · 1 question · 0 reject">
          <div>
            {[
              { att:"regime-verifier", v:"ENDORSE", t:"v3.0", time:"1h ago" },
              { att:"diversity-check", v:"ENDORSE", t:"v3.0", time:"1h ago" },
              { att:"regime-verifier", v:"ENDORSE", t:"v2.1", time:"2d ago" },
              { att:"diversity-check", v:"QUESTION", t:"v3.1", time:"4h ago" },
            ].map((a, i, arr) => {
              const tone = a.v === "ENDORSE" ? "var(--gold)" :
                           a.v === "QUESTION" ? "var(--warn)" : "var(--danger)";
              return (
                <div key={i} style={{
                  display:"flex", alignItems:"center", gap:10,
                  padding:"9px 14px",
                  borderBottom: i < arr.length-1 ? "1px solid var(--border-soft)" : "none",
                }}>
                  <span style={{
                    minWidth:80, display:"inline-flex", alignItems:"center", gap:5,
                    padding:"3px 7px", border:`1px solid ${tone}`, borderRadius:3,
                  }}>
                    <span style={{width:5, height:5, borderRadius:"50%", background:tone}}/>
                    <span className="mono" style={{fontSize:9.5, color:tone, letterSpacing:"0.14em", fontWeight:600}}>{a.v}</span>
                  </span>
                  <span className="mono" style={{fontSize:11, color:"var(--text-2)"}}>{a.att}</span>
                  <span className="mono" style={{marginLeft:"auto", fontSize:11, color:"var(--text)"}}>→ {a.t}</span>
                  <span className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>{a.time}</span>
                </div>
              );
            })}
          </div>
        </Card>

        {/* Anchor history */}
        <Card title="Anchor history" sub="3 events · total gas 0.0041 ETH" style={{gridColumn:"span 2"}}>
          <div>
            {[
              { kind:"Merkle", label:"Snapshot · btc-momentum-v3", tx:"0x2e1d…44a9", t:"2h ago",  gas:"0.0024 ETH" },
              { kind:"Mint",   label:"Identity NFT minted",         tx:"0xc0a4…f3b2", t:"4d 11h ago", gas:"0.0011 ETH" },
              { kind:"Commit", label:"SessionCommitment 01H8…RTZ",  tx:"0x4f8a…ee01", t:"4d 18h ago", gas:"0.0008 ETH" },
            ].map((e, i, arr) => (
              <div key={i} style={{
                display:"grid", gridTemplateColumns:"110px 1fr auto auto",
                alignItems:"center", gap:14, padding:"9px 16px",
                borderBottom: i < arr.length-1 ? "1px solid var(--border-soft)" : "none",
              }}>
                <span className="ulabel" style={{
                  fontSize:9.5, letterSpacing:"0.16em",
                  color: e.kind === "Merkle" ? "var(--info)" :
                         e.kind === "Mint" ? "var(--gold)" : "var(--text-2)",
                }}>{e.kind.toUpperCase()}</span>
                <span className="mono" style={{fontSize:11.5, color:"var(--text-2)"}}>{e.label}</span>
                <TxChip hash={e.tx}/>
                <span className="mono" style={{fontSize:11, color:"var(--text-3)", minWidth:90, textAlign:"right"}}>
                  {e.t} · {e.gas}
                </span>
              </div>
            ))}
          </div>
        </Card>

        {/* Trade history */}
        <TradeHistoryCard/>
      </div>
    )}
  </div>
);

// === Trade history (chain-verifiable per-trade ledger) ===
// Auditor view. Action filter pills mirror the eval detail Decisions pattern.
const TRADES = [
  { t:"2h 14m ago", action:"CLOSE", sym:"BTC", qty:"0.024", entry:"$67,420", exit:"$68,840", pnl:"+$34.08",  pnlPct:"+2.1%",  buyer:"0x7c2e…aa07", buyerKind:"human", tx:"0xa83e…f12d", anchor:"0x2e1d…44a9" },
  { t:"3h 02m ago", action:"BUY",   sym:"BTC", qty:"0.024", entry:"$67,420", exit:"—",        pnl:"open",     pnlPct:"—",      buyer:"0x7c2e…aa07", buyerKind:"human", tx:"0x91bc…aa72", anchor:"0x2e1d…44a9" },
  { t:"5h 41m ago", action:"CLOSE", sym:"BTC", qty:"0.018", entry:"$66,910", exit:"$67,820", pnl:"+$16.38",  pnlPct:"+1.4%",  buyer:"agent #14",   buyerKind:"agent", tx:"0x4f8a…dc11", anchor:"0x2e1d…44a9" },
  { t:"6h 22m ago", action:"BUY",   sym:"BTC", qty:"0.018", entry:"$66,910", exit:"—",        pnl:"open",     pnlPct:"—",      buyer:"agent #14",   buyerKind:"agent", tx:"0xc0a4…f3b2", anchor:"0x2e1d…44a9" },
  { t:"9h 18m ago", action:"SELL",  sym:"BTC", qty:"0.030", entry:"$68,240", exit:"$66,910", pnl:"-$39.90",  pnlPct:"-1.9%",  buyer:"0x4f8a…dc11", buyerKind:"human", tx:"0x55cd…ff19", anchor:"0x2e1d…44a9" },
  { t:"14h ago",    action:"CLOSE", sym:"BTC", qty:"0.012", entry:"$67,100", exit:"$68,240", pnl:"+$13.68",  pnlPct:"+1.7%",  buyer:"0x91bc…aa72", buyerKind:"human", tx:"0x1aa4…b201", anchor:"0x2e1d…44a9" },
  { t:"19h ago",    action:"BUY",   sym:"BTC", qty:"0.012", entry:"$67,100", exit:"—",        pnl:"open",     pnlPct:"—",      buyer:"0x91bc…aa72", buyerKind:"human", tx:"0x6e21…aa07", anchor:"0xprev…7a48" },
  { t:"1d 03h ago", action:"CLOSE", sym:"BTC", qty:"0.040", entry:"$65,840", exit:"$67,100", pnl:"+$50.40",  pnlPct:"+1.9%",  buyer:"agent #7",    buyerKind:"agent", tx:"0x8a99…12d3", anchor:"0xprev…7a48" },
  { t:"1d 09h ago", action:"BUY",   sym:"BTC", qty:"0.040", entry:"$65,840", exit:"—",        pnl:"open",     pnlPct:"—",      buyer:"agent #7",    buyerKind:"agent", tx:"0x77f9…1ed8", anchor:"0xprev…7a48" },
  { t:"1d 16h ago", action:"CLOSE", sym:"BTC", qty:"0.022", entry:"$64,920", exit:"$65,840", pnl:"+$20.24",  pnlPct:"+1.4%",  buyer:"0xc0a4…f3b2", buyerKind:"human", tx:"0xa07f…ee43", anchor:"0xprev…7a48" },
];

const TradeHistoryCard = () => {
  const counts = {
    all:   TRADES.length,
    BUY:   TRADES.filter(t => t.action === "BUY").length,
    SELL:  TRADES.filter(t => t.action === "SELL").length,
    CLOSE: TRADES.filter(t => t.action === "CLOSE").length,
  };
  const actionTone = {
    BUY:   { fg:"var(--gold)",   bd:"var(--gold-soft)",            bg:"var(--gold-bg)" },
    SELL:  { fg:"var(--danger)", bd:"rgba(255,77,77,0.40)",        bg:"rgba(255,77,77,0.10)" },
    CLOSE: { fg:"var(--info)",   bd:"rgba(95,168,255,0.40)",       bg:"rgba(95,168,255,0.10)" },
  };
  return (
    <Card
      title="Trade history"
      sub="178 trades on chain · last anchor 2h ago · receipt_kind=TradeBatch"
      right={<Btn variant="ghost" dense icon="ext">Export ledger</Btn>}
      style={{gridColumn:"span 2"}}
    >
      {/* Filter pills row + meta */}
      <div style={{
        padding:"10px 16px 8px", borderBottom:"1px solid var(--border-soft)",
        display:"flex", alignItems:"center", gap:8, flexWrap:"wrap",
      }}>
        {[
          ["all",   "All",   "all",   counts.all,   "var(--text-2)"],
          ["BUY",   "Buy",   "BUY",   counts.BUY,   "var(--gold)"],
          ["SELL",  "Sell",  "SELL",  counts.SELL,  "var(--danger)"],
          ["CLOSE", "Close", "CLOSE", counts.CLOSE, "var(--info)"],
        ].map(([k, label, _ig, count, col], i) => {
          const active = k === "all";
          return (
            <button key={k} style={{
              display:"inline-flex", alignItems:"center", gap:7,
              padding:"4px 10px", borderRadius:3,
              border:`1px solid ${active ? "var(--gold-soft)" : "var(--border-strong)"}`,
              background: active ? "var(--gold-bg)" : "transparent",
              color: active ? "var(--gold)" : "var(--text-2)",
              cursor:"pointer", fontFamily:"'Geist', sans-serif", fontSize:11.5, fontWeight:500,
            }}>
              <span style={{
                width:6, height:6, borderRadius:"50%",
                background: k === "all" ? "var(--text-3)" : col,
              }}/>
              <span>{label}</span>
              <span className="mono" style={{
                fontSize:10, color: active ? "var(--gold)" : "var(--text-3)",
                padding:"0 4px",
              }}>{count}</span>
            </button>
          );
        })}

        <span style={{width:1, height:18, background:"var(--border)", margin:"0 4px"}}/>

        {/* Buyer-kind filter */}
        <button style={{
          display:"inline-flex", alignItems:"center", gap:6,
          padding:"4px 10px", borderRadius:3,
          border:"1px solid var(--border-strong)", background:"transparent",
          color:"var(--text-2)", cursor:"pointer", fontSize:11.5, fontWeight:500,
        }}>
          <span style={{fontSize:11}}>Runner</span>
          <span className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>any</span>
          <Icon name="chevD" size={10} color="var(--text-3)" sw={2}/>
        </button>
        <button style={{
          display:"inline-flex", alignItems:"center", gap:6,
          padding:"4px 10px", borderRadius:3,
          border:"1px solid var(--border-strong)", background:"transparent",
          color:"var(--text-2)", cursor:"pointer", fontSize:11.5, fontWeight:500,
        }}>
          <span style={{fontSize:11}}>Window</span>
          <span className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>7d</span>
          <Icon name="chevD" size={10} color="var(--text-3)" sw={2}/>
        </button>

        <span style={{marginLeft:"auto"}} className="mono">
          <span style={{fontSize:10.5, color:"var(--text-3)"}}>
            net <span style={{color:"var(--gold)"}}>+$94.88</span> · 7d window
          </span>
        </span>
      </div>

      {/* Table */}
      <div>
        {/* Header */}
        <div style={{
          display:"grid",
          gridTemplateColumns:"100px 78px 50px 0.7fr 0.9fr 0.9fr 0.95fr 1fr 110px",
          alignItems:"center", gap:10,
          padding:"8px 16px",
          borderBottom:"1px solid var(--border-soft)",
        }}>
          {["Time","Action","Sym","Qty","Entry","Exit","P&L","Runner","Tx"].map((h, i) => (
            <div key={i} className="ulabel" style={{
              fontSize:9, letterSpacing:"0.2em", fontWeight:600,
              textAlign: i >= 3 && i <= 6 ? "right" : "left",
            }}>{h}</div>
          ))}
        </div>

        {/* Rows */}
        {TRADES.map((t, i) => {
          const tone = actionTone[t.action];
          const positive = t.pnl.startsWith("+");
          const open = t.pnl === "open";
          const pnlColor = open ? "var(--info)" : positive ? "var(--gold)" : "var(--danger)";
          const isAgent = t.buyerKind === "agent";
          return (
            <div key={i} style={{
              display:"grid",
              gridTemplateColumns:"100px 78px 50px 0.7fr 0.9fr 0.9fr 0.95fr 1fr 110px",
              alignItems:"center", gap:10,
              padding:"9px 16px",
              borderBottom: i < TRADES.length-1 ? "1px solid var(--border-soft)" : "none",
            }}>
              <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>{t.t}</span>
              <span style={{
                display:"inline-flex", alignItems:"center", gap:5,
                padding:"2px 7px", borderRadius:3,
                border:`1px solid ${tone.bd}`, background:tone.bg,
              }}>
                <span style={{width:4.5, height:4.5, borderRadius:"50%", background:tone.fg}}/>
                <span className="mono" style={{
                  fontSize:9.5, color:tone.fg, letterSpacing:"0.16em", fontWeight:600,
                }}>{t.action}</span>
              </span>
              <span className="mono" style={{fontSize:11.5, color:"var(--text-2)"}}>{t.sym}</span>
              <span className="mono" style={{fontSize:11.5, color:"var(--text)", textAlign:"right"}}>{t.qty}</span>
              <span className="mono" style={{fontSize:11.5, color:"var(--text-2)", textAlign:"right"}}>{t.entry}</span>
              <span className="mono" style={{fontSize:11.5, color: t.exit === "—" ? "var(--text-4)" : "var(--text-2)", textAlign:"right"}}>{t.exit}</span>
              <span style={{textAlign:"right", display:"flex", flexDirection:"column", alignItems:"flex-end", gap:1}}>
                <span className="mono" style={{fontSize:12, color:pnlColor, fontWeight:600}}>{t.pnl}</span>
                {!open && (
                  <span className="mono" style={{fontSize:9.5, color:"var(--text-3)"}}>{t.pnlPct}</span>
                )}
              </span>
              <span style={{display:"inline-flex", alignItems:"center", gap:6, minWidth:0}}>
                <span style={{
                  width:16, height:16, borderRadius: isAgent ? 3 : "50%",
                  background: isAgent ? "var(--gold-bg)" : "var(--surface-elev)",
                  border:`1px solid ${isAgent ? "var(--gold-soft)" : "var(--border-strong)"}`,
                  display:"flex", alignItems:"center", justifyContent:"center", flexShrink:0,
                }}>
                  {isAgent && <AgentIcon size={8}/>}
                </span>
                <span className="mono" style={{
                  fontSize:11, color: isAgent ? "var(--gold)" : "var(--text-2)",
                  overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap",
                }}>{t.buyer}</span>
              </span>
              <TxChip hash={t.tx} style={{justifySelf:"end"}}/>
            </div>
          );
        })}
      </div>

      {/* Footer — pagination + anchor pointer */}
      <div style={{
        padding:"10px 16px", borderTop:"1px solid var(--border-soft)",
        display:"flex", alignItems:"center", gap:10,
      }}>
        <span className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>
          Showing <span style={{color:"var(--text-2)"}}>10</span> of <span style={{color:"var(--text-2)"}}>178</span> · all anchored under
          <span style={{color:"var(--info)", marginLeft:6}}>Merkle 0x2e1d…44a9</span>
        </span>
        <div style={{marginLeft:"auto", display:"flex", gap:6}}>
          <Btn variant="ghost" dense>← Prev</Btn>
          <Btn variant="ghost" dense>Next →</Btn>
          <Btn variant="chip" dense icon="ext">Mantlescan</Btn>
        </div>
      </div>
    </Card>
  );
};

window.LineageIdentity = LineageIdentity;