// Frame — /marketplace/receipts/<tx-hash> · post-buy in-app
// + the standalone shareable card (OG / Twitter-card composition)
//
// Per §5.4: gen-art card + license token NFT + install steps for buyer's XVN
// + share composer pre-loaded.
// Per §3.1: every minted strategy gets a public URL that renders an
// "OG-card-perfect hero" — this is the screenshot moment.

const RECEIPT = {
  txHash: "0xa83e7c2efabb91d4eea7c2efbb91d4eef12d4eea83e7c2efabb91d4eea7c2efb",
  txShort: "0xa83e…b91d4e",
  receivedAt: "2026-05-26 14:42Z",
  receivedRel: "just now",
  buyer: "0x7c2e…aa07",
  // strategy bought
  strategyId: "btc-momentum-v3",
  strategyVer: "v3.0",
  creator: "@ed",
  creatorAddr: "0xa83e…f12d4",
  lineageBase: "btc-momentum-7a91-v3",
  ret30: "+47.2%",
  buyersH: 247, // includes you
  buyersA: 14,
  // license details
  licenseToken: "#0184",
  licenseContract: "0xCa55…22Be",
  manifestHash: "blake3:7f2b1ad…91c4",
  manifestCid: "bafybeib4xj…q2y7l",
  pricePaid: "49 USDC",
  feeAmount: "2.45 USDC",
  netToCreator: "46.55 USDC",
  // install state
  xvnDetected: true,
  xvnEndpoint: "localhost:3000",
  ingredients: [
    { name:"Claude Haiku 4.5",      kind:"model", installed:true },
    { name:"Birdeye MCP",           kind:"mcp",   installed:false },
    { name:"SOL Strategist skill",  kind:"skill", installed:false },
    { name:"Mantlescan MCP",        kind:"mcp",   installed:true },
  ],
};

const PurchaseReceipt = () => (
  <Frame>
    <SideNav active="marketplace"/>
    <main style={{display:"flex", flexDirection:"column", minWidth:0, overflow:"hidden"}}>
      <TopStatus breadcrumb={[
        { text:"MARKETPLACE" },
        { text:"receipt" },
        { text:RECEIPT.txShort, mono:true },
      ]}/>

      {/* Success header strip */}
      <div style={{
        padding:"18px 28px 16px", borderBottom:"1px solid var(--border)",
        background:
          "linear-gradient(90deg, rgba(0,230,118,0.10), rgba(0,230,118,0.02))",
        display:"flex", alignItems:"center", gap:18,
      }}>
        <div style={{
          width:44, height:44, borderRadius:"50%",
          background:"var(--gold-bg-strong)",
          border:"1px solid var(--gold)",
          display:"flex", alignItems:"center", justifyContent:"center", flexShrink:0,
        }}>
          <svg width="22" height="22" viewBox="0 0 22 22" fill="none"
            stroke="var(--gold)" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round">
            <path d="M4 11l5 5 9-11"/>
          </svg>
        </div>
        <div style={{flex:1, minWidth:0}}>
          <h1 style={{
            margin:0, fontSize:24, fontWeight:600, letterSpacing:"-0.02em", lineHeight:1.1,
          }}>You bought <span className="mono" style={{color:"var(--gold)"}}>{RECEIPT.strategyId}</span></h1>
          <div className="mono" style={{
            marginTop:6, fontSize:11.5, color:"var(--text-3)", letterSpacing:"0.01em",
          }}>
            <span><span style={{color:"var(--gold)"}}>{RECEIPT.pricePaid}</span> paid</span>
            <span style={{margin:"0 10px", color:"var(--text-4)"}}>·</span>
            <span>license <span style={{color:"var(--text-2)"}}>{RECEIPT.licenseToken}</span> minted</span>
            <span style={{margin:"0 10px", color:"var(--text-4)"}}>·</span>
            <span>{RECEIPT.netToCreator} → {RECEIPT.creator}</span>
            <span style={{margin:"0 10px", color:"var(--text-4)"}}>·</span>
            <TxChip hash={RECEIPT.txShort} tone="gold"/>
          </div>
        </div>
        <Btn variant="ghost" icon="ext">View on Mantlescan</Btn>
      </div>

      {/* Body: 3-col — license, install steps, share */}
      <div style={{
        flex:1, minHeight:0, padding:18, overflow:"auto",
        display:"grid", gridTemplateColumns:"320px 1fr 380px", gap:14,
      }}>

        {/* === LICENSE NFT — visual card === */}
        <Card title="License NFT" sub="non-transferable · ERC-721 on Mantle">
          <div style={{padding:"14px 14px 16px"}}>
            <div style={{position:"relative"}}>
              <GenArt seed={RECEIPT.lineageBase} size={290}
                style={{borderRadius:6, border:"1px solid var(--border)", width:"100%"}}/>
              {/* token id overlay */}
              <div style={{
                position:"absolute", top:8, left:8,
                padding:"3px 8px", borderRadius:3,
                background:"rgba(0,0,0,0.75)", backdropFilter:"blur(6px)",
              }}>
                <span className="mono" style={{
                  fontSize:10, color:"var(--text)", letterSpacing:"0.14em", fontWeight:600,
                }}>LICENSE {RECEIPT.licenseToken}</span>
              </div>
              <div style={{
                position:"absolute", bottom:8, right:8,
                padding:"3px 7px", borderRadius:3,
                background:"var(--gold-bg)", border:"1px solid var(--gold-soft)",
              }}>
                <span className="mono" style={{
                  fontSize:9.5, color:"var(--gold)", letterSpacing:"0.14em", fontWeight:600,
                }}>OWNED · YOU</span>
              </div>
            </div>

            {/* meta */}
            <div style={{marginTop:14, display:"flex", flexDirection:"column", gap:7}}>
              {[
                ["strategy", RECEIPT.strategyId, "gold"],
                ["version",  RECEIPT.strategyVer, "mono"],
                ["creator",  RECEIPT.creator,     "mono"],
                ["manifest", RECEIPT.manifestHash, "mono"],
                ["bundle",   `ipfs://${RECEIPT.manifestCid}`, "link"],
                ["paid",     `${RECEIPT.pricePaid} (5% fee · ${RECEIPT.feeAmount})`, "muted"],
                ["minted",   RECEIPT.receivedAt, "mono"],
              ].map(([k, v, t]) => (
                <div key={k} style={{display:"grid", gridTemplateColumns:"72px 1fr", gap:8, alignItems:"baseline"}}>
                  <span className="ulabel" style={{fontSize:9, letterSpacing:"0.16em"}}>{k}</span>
                  <span className="mono" style={{
                    fontSize:11, wordBreak:"break-all",
                    color: t === "gold" ? "var(--gold)"
                        : t === "link" ? "var(--info)"
                        : t === "muted" ? "var(--text-3)"
                        : "var(--text-2)",
                    textDecoration: t === "link" ? "underline dotted" : "none",
                    textUnderlineOffset:2,
                  }}>{v}</span>
                </div>
              ))}
            </div>
          </div>
        </Card>

        {/* === INSTALL STEPS === */}
        <Card
          title="Install in your XVN"
          sub={`detected at ${RECEIPT.xvnEndpoint} · 4 steps · sealed bundle auto-decrypts`}
          right={<Btn variant="primary">Install all</Btn>}
        >
          <div style={{padding:"4px 0"}}>
            <Step n={1} done title="XVN install detected"
              desc={
                <>Connected to your XVN at <span className="mono" style={{color:"var(--gold)"}}>{RECEIPT.xvnEndpoint}</span> · wallet matches <span className="mono" style={{color:"var(--text-2)"}}>{RECEIPT.buyer}</span>.</>
              }/>
            <Step n={2} active title="Decrypt sealed bundle"
              desc={
                <>Sealed bundle from IPFS — your license token authorizes decryption. About to fetch <span className="mono">{RECEIPT.manifestCid}</span>.</>
              }
              action={<Btn variant="primary" dense>Decrypt now</Btn>}
            />
            <Step n={3} title="Install missing ingredients"
              desc={
                <div>
                  <span>2 of 4 already installed in your XVN. Install the rest:</span>
                  <div style={{display:"flex", gap:6, marginTop:8, flexWrap:"wrap"}}>
                    {RECEIPT.ingredients.map((ing) => (
                      <span key={ing.name} style={{
                        display:"inline-flex", alignItems:"center", gap:5,
                        padding:"3px 8px", borderRadius:3,
                        border:`1px solid ${ing.installed ? "var(--gold-soft)" : "var(--warn)"}`,
                        background: ing.installed ? "var(--gold-bg)" : "rgba(255,176,32,0.08)",
                      }}>
                        {ing.installed
                          ? <Icon name="check" size={9} color="var(--gold)" sw={2}/>
                          : <Icon name="plus"  size={9} color="var(--warn)" sw={2}/>}
                        <span className="mono" style={{
                          fontSize:10.5, color: ing.installed ? "var(--gold)" : "var(--warn)",
                        }}>{ing.name}</span>
                        <span className="mono" style={{
                          fontSize:9, color:"var(--text-4)", letterSpacing:"0.14em",
                        }}>{ing.kind.toUpperCase()}</span>
                      </span>
                    ))}
                  </div>
                </div>
              }
              action={<Btn variant="chip" dense icon="plus">Install missing (2)</Btn>}
            />
            <Step n={4} title="Add to your Strategies and run paper-trade first"
              desc={
                <>Lands in <span className="mono" style={{color:"var(--text-2)"}}>Strategies / Marketplace · btc-momentum-v3</span>. Recommended: 7 days paper-trade with 2% risk cap before going live.</>
              }
              action={
                <div style={{display:"flex", gap:6}}>
                  <Btn variant="chip" dense>Add to strategies</Btn>
                  <Btn variant="ghost" dense icon="play">Open in XVN</Btn>
                </div>
              }
              last
            />
          </div>
        </Card>

        {/* === SHARE COMPOSER — preview of the OG card === */}
        <Card
          title="Share"
          sub="OG card pre-loaded · post to X / Farcaster / Discord"
        >
          <div style={{padding:"12px 14px"}}>
            {/* Embedded mini preview of the standalone shareable card */}
            <div style={{
              border:"1px solid var(--border)", borderRadius:6, overflow:"hidden",
              background:"#000", marginBottom:12,
            }}>
              <ShareableCardMini
                strategy={{
                  id: RECEIPT.strategyId,
                  ver: RECEIPT.strategyVer,
                  creator: RECEIPT.creator,
                  seed: RECEIPT.lineageBase,
                  ret30: RECEIPT.ret30,
                  buyersH: RECEIPT.buyersH,
                  buyersA: RECEIPT.buyersA,
                  paid: "$1,240",
                  price: "49 USDC",
                  verified: true,
                  x402: true,
                }}
                buyerStamp="just bought by 0x7c…aa07"
              />
            </div>

            {/* Caption editor */}
            <div className="ulabel" style={{fontSize:9, letterSpacing:"0.18em", marginBottom:6}}>CAPTION</div>
            <div style={{
              padding:"9px 11px",
              border:"1px solid var(--border-strong)", borderRadius:4,
              background:"var(--surface-elev)",
              fontSize:12.5, color:"var(--text)", lineHeight:1.55, minHeight:62,
            }}>
              I just bought <span className="mono" style={{color:"var(--gold)"}}>btc-momentum-v3</span> by {RECEIPT.creator} — running it now.<br/>
              <span style={{color:"var(--text-3)"}}>+47.2% in 30d · 247 humans + 14 agents already running it.</span>
            </div>

            {/* Suggested-edits */}
            <div style={{
              marginTop:8, padding:"6px 9px",
              border:"1px dashed var(--border-strong)", borderRadius:3,
            }}>
              <div className="ulabel" style={{fontSize:8.5, letterSpacing:"0.18em", marginBottom:4}}>
                SUGGESTED VARIANTS
              </div>
              <div style={{display:"flex", flexDirection:"column", gap:4}}>
                {[
                  "Just got handed +47% by an autonomous agent",
                  "247 humans run this. Me too now",
                  "@ed's btc-momentum is real. screenshot proof:",
                ].map((s) => (
                  <span key={s} className="mono" style={{
                    fontSize:10.5, color:"var(--text-3)", cursor:"pointer",
                    padding:"3px 0",
                  }}>↳ {s}</span>
                ))}
              </div>
            </div>

            {/* Targets */}
            <div className="ulabel" style={{fontSize:9, letterSpacing:"0.18em", margin:"14px 0 6px"}}>POST TO</div>
            <div style={{display:"grid", gridTemplateColumns:"1fr 1fr", gap:6}}>
              <Btn variant="chip" icon="ext" style={{justifyContent:"center"}}>X / Twitter</Btn>
              <Btn variant="chip" icon="ext" style={{justifyContent:"center"}}>Farcaster</Btn>
              <Btn variant="chip" icon="ext" style={{justifyContent:"center"}}>Discord</Btn>
              <Btn variant="chip" icon="copy" style={{justifyContent:"center"}}>Copy link</Btn>
            </div>
            <Btn variant="primary" style={{marginTop:10, width:"100%", justifyContent:"center"}}>
              Post to X
            </Btn>

            {/* Earnings hint for the creator */}
            <div style={{
              marginTop:14, padding:"9px 11px", borderRadius:4,
              background:"var(--gold-bg)", border:"1px solid var(--gold-soft)",
              display:"flex", alignItems:"center", gap:8,
            }}>
              <AgentIcon size={11}/>
              <span className="mono" style={{fontSize:11, color:"var(--gold)"}}>
                {RECEIPT.creator}'s XVN just got a +${(46.55).toFixed(2)} notification
              </span>
            </div>
          </div>
        </Card>
      </div>
    </main>
  </Frame>
);

// === Install step row ===
const Step = ({ n, title, desc, action, done = false, active = false, last = false }) => {
  const fg   = done ? "var(--gold)" : active ? "var(--gold)" : "var(--text-3)";
  const bd   = done ? "var(--gold)" : active ? "var(--gold)" : "var(--border-strong)";
  const bg   = done ? "var(--gold)" : active ? "var(--gold-bg)" : "transparent";
  return (
    <div style={{
      display:"grid", gridTemplateColumns:"38px 1fr auto", gap:14,
      padding:"14px 16px",
      borderBottom: last ? "none" : "1px solid var(--border-soft)",
      position:"relative",
    }}>
      {/* number / check */}
      <div style={{
        width:26, height:26, borderRadius:"50%",
        border:`1.5px solid ${bd}`, background:bg,
        display:"flex", alignItems:"center", justifyContent:"center",
        flexShrink:0,
      }}>
        {done ? (
          <svg width="13" height="13" viewBox="0 0 13 13" fill="none"
            stroke="#001A0A" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round">
            <path d="M2 7l3 3 6-7"/>
          </svg>
        ) : (
          <span className="mono" style={{fontSize:12, color:fg, fontWeight:600}}>{n}</span>
        )}
      </div>
      <div style={{minWidth:0}}>
        <div style={{
          fontSize:13.5, fontWeight:600, color: done ? "var(--text-3)" : "var(--text)",
          textDecoration: done ? "line-through" : "none",
        }}>{title}</div>
        <div style={{fontSize:12, color:"var(--text-2)", marginTop:5, lineHeight:1.55}}>
          {desc}
        </div>
      </div>
      <div style={{display:"flex", alignItems:"flex-start"}}>
        {action}
      </div>
    </div>
  );
};

// ─────────────────────────────────────────────────────────────
// Shareable card — the OG / Twitter-card composition.
// Standalone artboard renders this at 1200×630 (Twitter card aspect).
// Also embedded as a mini preview inside the receipt's share composer.
// ─────────────────────────────────────────────────────────────
const SHARE_DEMO_STRATEGY = {
  id: "btc-momentum-v3",
  ver: "v3.0",
  creator: "@ed",
  seed: "btc-momentum-7a91-v3",
  ret30: "+47.2%",
  ret30Period: "30D",
  buyersH: 247,
  buyersA: 14,
  paid: "$1,240",
  price: "49 USDC",
  verified: true,
  x402: true,
  promise: "BTC momentum with Claude regime detection.",
};

// Standalone wrapper — frame-shaped at 1200x630 with no XVN chrome
const ShareableCardFrame = ({ strategy = SHARE_DEMO_STRATEGY }) => (
  <div style={{
    width:"100%", height:"100%", background:"#000",
    display:"flex", alignItems:"center", justifyContent:"center",
    padding:0, overflow:"hidden",
  }}>
    <ShareableCard strategy={strategy}/>
  </div>
);

// The composition. Sized to fill its container.
const ShareableCard = ({ strategy }) => (
  <div style={{
    width:"100%", height:"100%",
    background:"#050505",
    display:"grid", gridTemplateColumns:"1fr 1fr",
    color:"var(--text)", overflow:"hidden",
    position:"relative",
  }}>
    {/* faint global tint */}
    <div style={{
      position:"absolute", inset:0,
      background:"radial-gradient(circle at 30% 30%, rgba(0,230,118,0.06), transparent 60%)",
      pointerEvents:"none",
    }}/>

    {/* LEFT — gen-art hero, full bleed */}
    <div style={{
      position:"relative", overflow:"hidden",
      borderRight:"1px solid var(--border)",
    }}>
      <GenArt seed={strategy.seed} size={1200}
        style={{
          width:"110%", height:"110%",
          position:"absolute", top:"-5%", left:"-5%",
          borderRadius:0,
        }}/>
      {/* corner marks */}
      <div style={{
        position:"absolute", top:24, left:24,
        display:"flex", flexDirection:"column", gap:6,
      }}>
        <BrandMark size={20}/>
        <span className="mono" style={{
          fontSize:10, color:"rgba(255,255,255,0.85)", letterSpacing:"0.2em",
          textShadow:"0 1px 4px rgba(0,0,0,0.6)",
        }}>XVN · MARKET</span>
      </div>
      {/* token id stamp */}
      <div style={{
        position:"absolute", bottom:24, left:24,
        padding:"6px 10px", borderRadius:3,
        background:"rgba(0,0,0,0.7)", backdropFilter:"blur(6px)",
        border:"1px solid rgba(255,255,255,0.18)",
      }}>
        <span className="mono" style={{
          fontSize:11, color:"rgba(255,255,255,0.95)", letterSpacing:"0.18em", fontWeight:600,
        }}>NFT #0043 · MANTLE</span>
      </div>
    </div>

    {/* RIGHT — info composition */}
    <div style={{
      padding:"38px 44px",
      display:"flex", flexDirection:"column", gap:14,
      position:"relative",
    }}>
      {/* top badges */}
      <div style={{display:"flex", alignItems:"center", gap:8, flexWrap:"wrap"}}>
        {strategy.verified && (
          <span style={{
            display:"inline-flex", alignItems:"center", gap:5,
            padding:"4px 9px", borderRadius:3,
            border:"1px solid var(--gold-soft)", background:"var(--gold-bg)",
          }}>
            <Icon name="check" size={11} color="var(--gold)" sw={2.2}/>
            <span className="mono" style={{fontSize:10, color:"var(--gold)", letterSpacing:"0.18em", fontWeight:600}}>
              VERIFIED
            </span>
          </span>
        )}
        {strategy.x402 && (
          <span style={{
            display:"inline-flex", alignItems:"center", gap:6,
            padding:"4px 9px", borderRadius:3,
            border:"1px solid var(--gold-soft)", background:"var(--gold-bg)",
          }}>
            <AgentIcon size={11}/>
            <span className="mono" style={{fontSize:10, color:"var(--gold)", letterSpacing:"0.18em", fontWeight:600}}>
              x402 · ACCEPTS AGENTS
            </span>
          </span>
        )}
      </div>

      {/* name + creator */}
      <div>
        <h1 style={{
          margin:0, fontSize:44, fontWeight:600, letterSpacing:"-0.025em", lineHeight:1.02,
          fontFamily:"'Geist Mono', monospace",
        }}>{strategy.id}</h1>
        <div style={{display:"flex", alignItems:"center", gap:10, marginTop:8}}>
          <span className="mono" style={{fontSize:15, color:"var(--text-2)"}}>by {strategy.creator}</span>
          <span style={{color:"var(--text-4)"}}>·</span>
          <span className="mono" style={{fontSize:13, color:"var(--text-3)"}}>{strategy.ver}</span>
        </div>
        {strategy.promise && (
          <p style={{
            margin:"12px 0 0", fontSize:15, color:"var(--text)", lineHeight:1.4,
          }}>{strategy.promise}</p>
        )}
      </div>

      {/* big return % */}
      <div style={{
        marginTop:8,
        padding:"14px 0 12px",
        borderTop:"1px solid var(--border)",
        borderBottom:"1px solid var(--border)",
        display:"grid", gridTemplateColumns:"auto 1fr", gap:18, alignItems:"end",
      }}>
        <div>
          <div className="ulabel" style={{fontSize:10, letterSpacing:"0.2em", marginBottom:4}}>
            {strategy.ret30Period || "30D"} RETURN
          </div>
          <div className="mono" style={{
            fontSize:64, fontWeight:600, color:"var(--gold)", letterSpacing:"-0.035em",
            lineHeight:1,
          }}>{strategy.ret30}</div>
        </div>
        <div style={{display:"flex", flexDirection:"column", gap:6, paddingBottom:6}}>
          <div className="ulabel" style={{fontSize:10, letterSpacing:"0.2em"}}>
            RUN BY
          </div>
          <div style={{display:"flex", alignItems:"baseline", gap:6, flexWrap:"wrap"}}>
            <span className="mono" style={{fontSize:22, color:"var(--text)", fontWeight:600}}>
              {strategy.buyersH}
            </span>
            <span className="mono" style={{fontSize:13, color:"var(--text-3)"}}>humans</span>
            <span style={{color:"var(--text-4)", fontSize:13}}>+</span>
            <span style={{
              display:"inline-flex", alignItems:"center", gap:5,
              padding:"2px 8px", borderRadius:3,
              background:"var(--gold-bg)", border:"1px solid var(--gold-soft)",
            }}>
              <AgentIcon size={11}/>
              <span className="mono" style={{fontSize:14, color:"var(--gold)", fontWeight:600}}>
                {strategy.buyersA}
              </span>
              <span className="mono" style={{fontSize:11, color:"var(--gold)"}}>agents</span>
            </span>
          </div>
        </div>
      </div>

      {/* bottom: price + URL + QR */}
      <div style={{
        marginTop:"auto",
        display:"grid", gridTemplateColumns:"1fr auto", gap:16, alignItems:"end",
      }}>
        <div>
          <div className="ulabel" style={{fontSize:10, letterSpacing:"0.2em", marginBottom:4}}>
            BUY · USDC
          </div>
          <div style={{display:"flex", alignItems:"baseline", gap:10}}>
            <span className="mono" style={{fontSize:30, color:"var(--text)", fontWeight:600, letterSpacing:"-0.02em"}}>
              {strategy.price}
            </span>
            <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>
              perpetual · {strategy.paid} paid to creator
            </span>
          </div>
          <div className="mono" style={{
            marginTop:12, fontSize:12, color:"var(--gold)", letterSpacing:"0.04em",
          }}>
            xvn.market/lineage/{strategy.id}
          </div>
        </div>
        <QrCode/>
      </div>
    </div>
  </div>
);

// Mini preview — used inside the share composer
const ShareableCardMini = ({ strategy, buyerStamp }) => (
  <div style={{position:"relative"}}>
    {/* 1200x630 aspect — render at full width of its container */}
    <div style={{
      aspectRatio:"1200 / 630", width:"100%", overflow:"hidden",
      border:"1px solid var(--border)", borderRadius:6, background:"#050505",
    }}>
      <ShareableCard strategy={strategy}/>
    </div>
    {buyerStamp && (
      <div style={{
        position:"absolute", top:6, right:6,
        padding:"3px 7px", borderRadius:3,
        background:"rgba(0,0,0,0.75)", backdropFilter:"blur(6px)",
        border:"1px solid rgba(255,255,255,0.15)",
      }}>
        <span className="mono" style={{
          fontSize:9.5, color:"rgba(255,255,255,0.9)", letterSpacing:"0.12em",
        }}>{buyerStamp}</span>
      </div>
    )}
    {/* size hint */}
    <div style={{
      marginTop:6, display:"flex", alignItems:"center", justifyContent:"space-between",
    }}>
      <span className="mono" style={{fontSize:9.5, color:"var(--text-3)", letterSpacing:"0.16em"}}>
        OG CARD · 1200 × 630
      </span>
      <span className="mono" style={{fontSize:9.5, color:"var(--text-3)"}}>
        twitter / farcaster / opengraph
      </span>
    </div>
  </div>
);

// Faux QR — visual placeholder, deterministic from a seed
const QrCode = ({ size = 78 }) => {
  const cells = 9;
  const rng = bc2Rng(bc2Hash("xvn-qr-btc-momentum-v3"));
  // 3 finder patterns in corners + random middle cells
  const grid = Array.from({length: cells}, (_, y) =>
    Array.from({length: cells}, (_, x) => {
      const finder =
        (x < 3 && y < 3) || (x >= cells-3 && y < 3) || (x < 3 && y >= cells-3);
      if (finder) {
        const dx = x < 3 ? x : x - (cells-3);
        const dy = y < 3 ? y : y - (cells-3);
        if (dx === 0 || dy === 0 || dx === 2 || dy === 2) return 1;
        if (dx === 1 && dy === 1) return 1;
        return 0;
      }
      return rng() > 0.55 ? 1 : 0;
    })
  );
  const cell = size / cells;
  return (
    <div style={{
      width:size+10, height:size+10, padding:5,
      background:"var(--gold)", borderRadius:6,
      display:"flex", alignItems:"center", justifyContent:"center",
      flexShrink:0,
    }}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
        {grid.map((row, y) =>
          row.map((v, x) => v ? (
            <rect key={`${x}-${y}`}
              x={x*cell} y={y*cell} width={cell} height={cell}
              fill="#001A0A"/>
          ) : null)
        )}
      </svg>
    </div>
  );
};

Object.assign(window, { PurchaseReceipt, ShareableCardFrame, ShareableCard });
