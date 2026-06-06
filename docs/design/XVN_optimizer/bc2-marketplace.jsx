// Frame — /marketplace · vibetrader-facing browse
//
// Replaces the operator 2×2 panel grid with a botspot-style list.
// Above the fold: counter flex, segmented sort/filter, tag chips, search,
// list with gen-art · name · @creator · asset pills · 30d return % · sparkline
// · sharpe (muted) · buyers (humans + agents) · price · Buy/Run free.
// Left rail: leaderboard presets (each a shareable URL).
//
// Auditor surfaces (anchor, attest, mint missing NFTs) live elsewhere now.

const STRATEGIES = [
  { id:"sol-strategist-pro", lineage:"sol-strategist", creator:"@vibesharpe", ver:"v4.2",
    assets:["SOL"], model:"Claude", style:"Day",
    ret30:"+89.4%", sharpe:"+1.84", buyersH:412, buyersA:38,
    price:"79 USDC", priceKind:"buy", verified:true, x402:true,
    trend:true, lineageBase:"sol-strategist-12fa" },
  { id:"btc-momentum-v3", lineage:"btc-momentum", creator:"@ed", ver:"v3.0",
    assets:["BTC"], model:"Claude", style:"Swing",
    ret30:"+47.2%", sharpe:"+1.31", buyersH:247, buyersA:14,
    price:"49 USDC", priceKind:"buy", verified:true, x402:true,
    trend:true, lineageBase:"btc-momentum-7a91" },
  { id:"meme-radar", lineage:"meme-radar", creator:"@degenray", ver:"v1.6",
    assets:["SOL"], model:"GPT", style:"Day",
    ret30:"+124.8%", sharpe:"+0.62", buyersH:89, buyersA:22,
    price:"Open", priceKind:"free", verified:false, x402:true,
    trend:true, lineageBase:"meme-radar-de44" },
  { id:"eth-mr-v4", lineage:"eth-mr", creator:"@kaori", ver:"v4.1",
    assets:["ETH"], model:"Claude", style:"Swing",
    ret30:"+18.6%", sharpe:"+1.62", buyersH:156, buyersA:7,
    price:"39 USDC", priceKind:"buy", verified:true, x402:false,
    trend:false, lineageBase:"eth-mr-3b22" },
  { id:"multi-asset-rotation", lineage:"multi-asset", creator:"@quantnext", ver:"v2.0",
    assets:["BTC","ETH","SOL"], model:"Claude", style:"Swing",
    ret30:"+22.4%", sharpe:"+1.41", buyersH:203, buyersA:18,
    price:"99 USDC", priceKind:"buy", verified:true, x402:true,
    trend:false, lineageBase:"multi-asset-9c10" },
  { id:"btc-grid-v2", lineage:"btc-grid", creator:"@dca-anon", ver:"v2.3",
    assets:["BTC"], model:"Claude", style:"Day",
    ret30:"+31.4%", sharpe:"+1.08", buyersH:134, buyersA:9,
    price:"69 USDC", priceKind:"buy", verified:true, x402:false,
    trend:false, lineageBase:"btc-grid-6f5b" },
  { id:"eth-swing", lineage:"eth-swing", creator:"@solyana", ver:"v1.4",
    assets:["ETH"], model:"Gemini", style:"Swing",
    ret30:"+22.1%", sharpe:"+1.12", buyersH:67, buyersA:3,
    price:"59 USDC", priceKind:"buy", verified:false, x402:false,
    trend:false, lineageBase:"eth-swing-aa07" },
  { id:"zksync-airdrop", lineage:"zksync-airdrop", creator:"@yieldfarmer", ver:"v0.8",
    assets:["ETH"], model:"GPT", style:"Long",
    ret30:"+8.1%", sharpe:"+0.78", buyersH:45, buyersA:2,
    price:"Open", priceKind:"free", verified:false, x402:false,
    trend:false, lineageBase:"zksync-airdrop-d3b1" },
  { id:"doge-vol", lineage:"doge-vol", creator:"@memewhale", ver:"v1.0",
    assets:["DOGE"], model:"GPT", style:"Day",
    ret30:"-2.3%", sharpe:"-0.18", buyersH:12, buyersA:0,
    price:"29 USDC", priceKind:"buy", verified:false, x402:false,
    trend:false, lineageBase:"doge-vol-9911" },
  { id:"mantle-native-yield", lineage:"mantle-yield", creator:"@nodes", ver:"v2.1",
    assets:["ETH"], model:"Claude", style:"Long",
    ret30:"+9.7%", sharpe:"+1.04", buyersH:78, buyersA:4,
    price:"39 USDC", priceKind:"buy", verified:true, x402:false,
    trend:false, lineageBase:"mantle-yield-6622" },
];

const LEADERBOARDS = [
  { key:"trending",   label:"Trending",         hint:"weighted by 24h velocity × return",  count:1247, active:true },
  { key:"sol-7d",     label:"Top on SOL · 7d",  hint:"asset=SOL · 7d window",              count:142 },
  { key:"claude",     label:"Top with Claude",  hint:"model=Claude",                       count:431 },
  { key:"agents",     label:"Most agent-bought",hint:"sort by 🤖 purchases",               count:88 },
  { key:"new",        label:"Newest 24h",       hint:"minted < 24h ago",                   count:23 },
  { key:"cloned",     label:"Most cloned",      hint:"clones-of edges",                    count:64 },
  { key:"free",       label:"Free-tier breakouts", hint:"Tier A · ret > 25%",              count:17 },
];

const TAG_GROUPS = [
  { key:"asset",  active:[], options:["SOL","BTC","ETH","DOGE","Equities","Memes"] },
  { key:"model",  active:[],   options:["Claude","GPT","Gemini","Llama"] },
  { key:"style",  active:[], options:["Long","Long/Short","Day","Swing"] },
];

// === Marketplace browse component (replaces operator 2×2) ===
const MarketplaceBrowse = () => (
  <Frame>
    <SideNav active="marketplace"/>
    <main style={{display:"flex", flexDirection:"column", minWidth:0, overflow:"hidden"}}>
      <TopStatus breadcrumb={[
        { text:"MARKETPLACE" },
      ]}/>

      {/* Counter flex / promise / segmented */}
      <div style={{
        padding:"20px 28px 18px", borderBottom:"1px solid var(--border)",
        display:"flex", justifyContent:"space-between", alignItems:"flex-end", gap:24,
      }}>
        <div style={{minWidth:0, maxWidth:780}}>
          <h1 style={{
            margin:0, fontSize:24, fontWeight:600, letterSpacing:"-0.025em", lineHeight:1.15,
          }}>Buy a strategy. Run it. Or share yours and get paid.</h1>
          <div className="mono" style={{
            marginTop:10, fontSize:11.5, color:"var(--text-3)", letterSpacing:"0.01em",
            display:"flex", alignItems:"center", gap:0, flexWrap:"wrap",
          }}>
            <span><span style={{color:"var(--text-2)"}}>1,247</span> strategies</span>
            <span style={{margin:"0 10px", color:"var(--text-4)"}}>·</span>
            <span><span style={{color:"var(--gold)"}}>$34,820</span> paid this week</span>
            <span style={{margin:"0 10px", color:"var(--text-4)"}}>·</span>
            <span style={{display:"inline-flex", alignItems:"center", gap:5}}>
              <AgentIcon size={11}/>
              <span><span style={{color:"var(--text-2)"}}>218</span> agent purchases</span>
            </span>
            <span style={{margin:"0 10px", color:"var(--text-4)"}}>·</span>
            <span><span style={{color:"var(--text-2)"}}>64</span> minted in 24h</span>
          </div>
        </div>
        <div style={{display:"flex", gap:8, alignItems:"center", flexShrink:0}}>
          <Btn variant="ghost" icon="ext">Share</Btn>
          <Btn variant="primary" icon="plus">Share your strategy</Btn>
        </div>
      </div>

      {/* Toolbar: segmented + search + sort + Filters button */}
      <div style={{position:"relative", borderBottom:"1px solid var(--border)"}}>
        <div style={{
          padding:"14px 28px 12px",
          display:"flex", alignItems:"center", gap:12, flexWrap:"wrap",
        }}>
          <Segmented options={[
            { key:"trending", label:"Trending" },
            { key:"new",      label:"New" },
            { key:"mine",     label:"Mine" },
          ]} active="trending"/>

          {/* Search */}
          <div style={{
            flex:1, minWidth:240, maxWidth:380,
            display:"flex", alignItems:"center", gap:8, padding:"6px 10px",
            border:"1px solid var(--border-strong)", borderRadius:4,
            background:"var(--surface-elev)",
          }}>
            <Icon name="search" size={13} color="var(--text-3)"/>
            <span className="mono" style={{fontSize:12, color:"var(--text-3)"}}>name · creator · tag</span>
            <span style={{
              marginLeft:"auto", padding:"1px 6px",
              border:"1px solid var(--border-strong)", borderRadius:3,
              fontFamily:"'Geist Mono', monospace", fontSize:9.5, color:"var(--text-3)",
              letterSpacing:"0.06em",
            }}>/</span>
          </div>

          {/* Sort dropdown */}
          <FilterButton
            label="Sort"
            value="30d return"
            icon="chevD"
          />

          <span style={{width:1, height:22, background:"var(--border)"}}/>

          {/* Single Filters button — opens the drawer */}
          <FilterButton label="Filters"  value="" count={4} open icon="chevR"/>

          <div style={{marginLeft:"auto"}}>
            <Btn variant="ghost" dense icon="ext">Save view</Btn>
          </div>
        </div>

        {/* Applied filter chips row */}
        <div style={{
          padding:"4px 28px 12px",
          display:"flex", alignItems:"center", gap:7, flexWrap:"wrap",
        }}>
          <span className="ulabel" style={{fontSize:9, letterSpacing:"0.2em"}}>APPLIED</span>
          <RemovableChip>Asset: BTC</RemovableChip>
          <RemovableChip>Asset: SOL</RemovableChip>
          <RemovableChip>Model: Claude</RemovableChip>
          <RemovableChip>Verified only</RemovableChip>
          <span style={{
            color:"var(--text-3)", fontSize:11.5, marginLeft:4, cursor:"pointer",
            textDecoration:"underline dotted", textUnderlineOffset:3,
          }}>Clear all</span>
          <span style={{marginLeft:"auto"}} className="mono">
            <span style={{fontSize:11, color:"var(--text-3)"}}>
              <span style={{color:"var(--text-2)"}}>342</span> matches
            </span>
          </span>
        </div>
      </div>

      {/* Body: left rail (leaderboards) + list, drawer overlays */}
      <div style={{
        flex:1, minHeight:0, display:"grid",
        gridTemplateColumns:"232px 1fr", overflow:"hidden",
        position:"relative",
      }}>

        {/* LEFT RAIL — leaderboard presets */}
        <aside style={{
          borderRight:"1px solid var(--border)", padding:"16px 14px",
          display:"flex", flexDirection:"column", gap:14, overflow:"hidden",
        }}>
          <div>
            <div style={{
              display:"flex", alignItems:"center", justifyContent:"space-between",
              marginBottom:8,
            }}>
              <span className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em"}}>LEADERBOARDS</span>
              <span className="mono" style={{fontSize:10, color:"var(--text-4)"}}>shareable URLs</span>
            </div>
            <div style={{display:"flex", flexDirection:"column"}}>
              {LEADERBOARDS.map((l) => {
                const isActive = l.active;
                return (
                  <div key={l.key} style={{
                    padding:"8px 10px", margin:"0 -8px",
                    borderRadius:4, cursor:"pointer",
                    background: isActive ? "var(--gold-bg)" : "transparent",
                    border: isActive ? "1px solid var(--gold-soft)" : "1px solid transparent",
                  }}>
                    <div style={{display:"flex", alignItems:"center", gap:8}}>
                      <span style={{
                        fontSize:12.5, fontWeight: isActive ? 600 : 500,
                        color: isActive ? "var(--gold)" : "var(--text)",
                      }}>{l.label}</span>
                      <span className="mono" style={{
                        marginLeft:"auto", fontSize:10, color:"var(--text-3)",
                      }}>{l.count}</span>
                    </div>
                    <div className="mono" style={{
                      fontSize:9.5, color:"var(--text-3)", marginTop:3, letterSpacing:"0.02em",
                    }}>{l.hint}</div>
                  </div>
                );
              })}
            </div>
          </div>

          <div style={{
            marginTop:"auto", padding:"10px 12px",
            border:"1px dashed var(--border-strong)", borderRadius:5,
          }}>
            <div className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em", marginBottom:6}}>
              CHAIN OPS
            </div>
            <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", lineHeight:1.5}}>
              Anchor · mint missing · attesters → in <span style={{color:"var(--text-2)"}}>Settings → Chain ops</span>
            </div>
          </div>
        </aside>

        {/* LIST */}
        <div style={{overflow:"auto", padding:"0 0 8px"}}>
          {/* List header row */}
          <div style={{
            display:"grid",
            gridTemplateColumns:"56px 1.8fr 0.75fr 1.1fr 1.05fr 0.6fr 0.85fr 110px",
            alignItems:"center", gap:14,
            padding:"10px 22px",
            borderBottom:"1px solid var(--border-soft)",
            position:"sticky", top:0, background:"#000", zIndex:1,
          }}>
            {["","Strategy","Assets","30d return","Buyers","Sharpe","Price",""].map((h, i) => (
              <div key={i} className="ulabel" style={{
                fontSize:9, letterSpacing:"0.2em", fontWeight:600,
                textAlign: i === 3 || i === 5 ? "right" : "left",
              }}>{h}</div>
            ))}
          </div>

      {STRATEGIES.map((s, i) => (
            <StrategyRow key={s.id} s={s} index={i}/>
          ))}
        </div>

        {/* FILTER DRAWER — overlays the list area (right side) */}
        <FilterDrawer/>
      </div>
    </main>
  </Frame>
);

// ── small components ──

// Filter button — toolbar entry that opens an inline popover.
// Shows: label, optional value summary, optional count badge, chev.
// `open` styles the button as currently expanded.
const FilterButton = ({ label, value, count, icon = "chevD", open = false }) => (
  <button style={{
    display:"inline-flex", alignItems:"center", gap:7,
    padding:"5px 9px 5px 11px", borderRadius:4,
    border: open ? "1px solid var(--gold-soft)" : "1px solid var(--border-strong)",
    background: open ? "var(--gold-bg)" : "var(--surface-elev)",
    color: open ? "var(--gold)" : "var(--text-2)",
    cursor:"pointer",
    fontFamily:"'Geist', sans-serif", fontSize:12, lineHeight:1,
    position:"relative",
  }}>
    <span style={{fontWeight:500}}>{label}</span>
    {(value || count !== undefined) && (
      <span style={{
        display:"inline-flex", alignItems:"center", gap:5,
        paddingLeft:7, marginLeft:1,
        borderLeft:`1px solid ${open ? "var(--gold-soft)" : "var(--border)"}`,
      }}>
        {count !== undefined && (
          <span style={{
            padding:"1px 5px", borderRadius:8, minWidth:14, textAlign:"center",
            background: open ? "var(--gold)" : "var(--border-strong)",
            color: open ? "#001A0A" : "var(--text)",
            fontFamily:"'Geist Mono', monospace", fontSize:9.5, fontWeight:700,
            lineHeight:1.3,
          }}>{count}</span>
        )}
        {value && (
          <span className="mono" style={{
            fontSize:11, color: open ? "var(--gold)" : "var(--text-3)",
            maxWidth:120, overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap",
          }}>{value}</span>
        )}
      </span>
    )}
    <Icon name={icon} size={10} color="currentColor" sw={2}/>
  </button>
);

// Applied filter chip with × to remove
const RemovableChip = ({ children }) => (
  <span style={{
    display:"inline-flex", alignItems:"center", gap:6,
    padding:"3px 4px 3px 9px", borderRadius:3,
    border:"1px solid var(--gold-soft)",
    background:"var(--gold-bg)",
    color:"var(--gold)",
    fontFamily:"'Geist Mono', monospace", fontSize:10.5, letterSpacing:"0.04em",
    cursor:"default",
  }}>
    {children}
    <span style={{
      display:"inline-flex", alignItems:"center", justifyContent:"center",
      width:14, height:14, borderRadius:2,
      cursor:"pointer",
    }}>
      <svg width="9" height="9" viewBox="0 0 9 9" fill="none" stroke="currentColor"
        strokeWidth="1.6" strokeLinecap="round">
        <path d="M1.5 1.5l6 6M7.5 1.5l-6 6"/>
      </svg>
    </span>
  </span>
);

// Asset catalogue — exposes the "as providers grow" pressure
const ASSETS = [
  { group:"Crypto · majors",
    items: [
      { sym:"BTC",   name:"Bitcoin",       count:312, selected:true },
      { sym:"ETH",   name:"Ethereum",      count:289, selected:false },
      { sym:"SOL",   name:"Solana",        count:204, selected:true },
      { sym:"MATIC", name:"Polygon",       count:48 },
      { sym:"AVAX",  name:"Avalanche",     count:33 },
    ]},
  { group:"Crypto · L2 & memes",
    items: [
      { sym:"ARB",   name:"Arbitrum",      count:62 },
      { sym:"OP",    name:"Optimism",      count:51 },
      { sym:"BASE",  name:"Base",          count:74 },
      { sym:"MNT",   name:"Mantle",        count:29 },
      { sym:"DOGE",  name:"Dogecoin",      count:41 },
      { sym:"WIF",   name:"dogwifhat",     count:18 },
      { sym:"PEPE",  name:"Pepe",          count:22 },
    ]},
  { group:"Equities",
    items: [
      { sym:"SPY",   name:"S&P 500 ETF",   count:14 },
      { sym:"QQQ",   name:"Nasdaq-100",    count:11 },
      { sym:"NVDA",  name:"NVIDIA",        count:23 },
      { sym:"TSLA",  name:"Tesla",         count:19 },
    ]},
  { group:"FX",
    items: [
      { sym:"EUR/USD", name:"Euro / USD",  count:6 },
      { sym:"USD/JPY", name:"USD / Yen",   count:4 },
    ]},
];

// Inline filter drawer — slides in from the right edge of the list area.
// Holds every filter category at once with breathing room: sort, assets
// (grouped + searchable), models, style, verified/x402 toggles, price range,
// min buyers. Static demo state: shown open.
const FilterDrawer = () => (
  <>
    {/* Dim backdrop over the list area only (not the side rails) */}
    <div style={{
      position:"absolute", inset:0, gridColumnStart:2,
      background:"rgba(0,0,0,0.55)", backdropFilter:"blur(1.5px)",
      zIndex:4,
    }}/>

    {/* Drawer panel — anchored to the right */}
    <div style={{
      position:"absolute", top:0, right:0, bottom:0, width:400,
      background:"#070707",
      borderLeft:"1px solid var(--border-strong)",
      boxShadow:"-20px 0 40px rgba(0,0,0,0.6)",
      zIndex:5,
      display:"flex", flexDirection:"column",
    }}>
      {/* Header */}
      <div style={{
        padding:"14px 18px",
        borderBottom:"1px solid var(--border)",
        display:"flex", alignItems:"center", justifyContent:"space-between",
      }}>
        <div>
          <div style={{fontSize:14.5, fontWeight:600, color:"var(--text)"}}>Filter strategies</div>
          <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginTop:2}}>
            <span style={{color:"var(--gold)"}}>4 filters active</span> · 342 of 1,247 match
          </div>
        </div>
        <button style={{
          width:28, height:28, borderRadius:4,
          border:"1px solid var(--border-strong)", background:"transparent",
          color:"var(--text-2)", cursor:"pointer",
          display:"flex", alignItems:"center", justifyContent:"center",
        }}>
          <svg width="11" height="11" viewBox="0 0 11 11" fill="none" stroke="currentColor"
            strokeWidth="1.6" strokeLinecap="round">
            <path d="M2 2l7 7M9 2l-7 7"/>
          </svg>
        </button>
      </div>

      {/* Body */}
      <div style={{flex:1, minHeight:0, overflowY:"auto"}}>

        {/* SORT */}
        <DrawerSection title="Sort by">
          <div style={{display:"flex", flexDirection:"column", gap:6}}>
            {[
              ["30d return", true],
              ["Sharpe", false],
              ["Buyers (humans + agents)", false],
              ["Most cloned", false],
              ["Newest", false],
            ].map(([label, active]) => (
              <label key={label} style={{
                display:"flex", alignItems:"center", gap:9,
                padding:"6px 4px", cursor:"pointer",
              }}>
                <span style={{
                  width:13, height:13, borderRadius:"50%",
                  border:`1.5px solid ${active ? "var(--gold)" : "var(--border-strong)"}`,
                  background: active ? "var(--gold-bg)" : "transparent",
                  display:"flex", alignItems:"center", justifyContent:"center",
                }}>
                  {active && <span style={{width:5, height:5, borderRadius:"50%", background:"var(--gold)"}}/>}
                </span>
                <span style={{fontSize:12.5, color: active ? "var(--text)" : "var(--text-2)"}}>{label}</span>
              </label>
            ))}
          </div>
        </DrawerSection>

        {/* ASSETS — search + grouped checklist */}
        <DrawerSection title="Assets" subCount="2 selected · 6 groups">
          <div style={{
            display:"flex", alignItems:"center", gap:7,
            padding:"5px 9px", marginBottom:8,
            border:"1px solid var(--border-strong)", borderRadius:3,
            background:"var(--surface-elev)",
          }}>
            <Icon name="search" size={11} color="var(--text-3)"/>
            <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>filter assets…</span>
          </div>
          {ASSETS.map((g, gi) => (
            <div key={g.group} style={{marginBottom: gi < ASSETS.length - 1 ? 6 : 0}}>
              <div style={{
                padding:"6px 0 4px",
                display:"flex", alignItems:"baseline", justifyContent:"space-between",
              }}>
                <span className="ulabel" style={{fontSize:9, letterSpacing:"0.18em"}}>{g.group}</span>
                <span className="mono" style={{fontSize:9.5, color:"var(--text-4)"}}>{g.items.length}</span>
              </div>
              {g.items.map((a) => (
                <div key={a.sym} style={{
                  display:"grid", gridTemplateColumns:"18px 64px 1fr auto",
                  alignItems:"center", gap:10,
                  padding:"5px 6px", borderRadius:3,
                  cursor:"pointer",
                  background: a.selected ? "var(--gold-bg)" : "transparent",
                }}>
                  <span style={{
                    width:13, height:13, borderRadius:2,
                    border:`1px solid ${a.selected ? "var(--gold)" : "var(--border-strong)"}`,
                    background: a.selected ? "var(--gold)" : "transparent",
                    display:"flex", alignItems:"center", justifyContent:"center",
                  }}>
                    {a.selected && (
                      <svg width="9" height="9" viewBox="0 0 9 9" fill="none"
                        stroke="#001A0A" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                        <path d="M1.5 4.5L4 7l4-5"/>
                      </svg>
                    )}
                  </span>
                  <span className="mono" style={{
                    fontSize:11.5, fontWeight:600,
                    color: a.selected ? "var(--gold)" : "var(--text)",
                  }}>{a.sym}</span>
                  <span style={{
                    fontSize:11.5, color: a.selected ? "var(--text)" : "var(--text-2)",
                  }}>{a.name}</span>
                  <span className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>{a.count}</span>
                </div>
              ))}
            </div>
          ))}
        </DrawerSection>

        {/* MODELS */}
        <DrawerSection title="Models" subCount="1 selected">
          {[
            { name:"Claude · Haiku 4.5",  count:431, selected:true  },
            { name:"Claude · Sonnet 4.5", count:118, selected:false },
            { name:"GPT-5",               count:312, selected:false },
            { name:"Gemini 3 Pro",        count:204, selected:false },
            { name:"Llama 4",             count:48,  selected:false },
          ].map((m) => (
            <div key={m.name} style={{
              display:"grid", gridTemplateColumns:"18px 1fr auto",
              alignItems:"center", gap:10,
              padding:"5px 6px", borderRadius:3,
              cursor:"pointer",
              background: m.selected ? "var(--gold-bg)" : "transparent",
            }}>
              <span style={{
                width:13, height:13, borderRadius:2,
                border:`1px solid ${m.selected ? "var(--gold)" : "var(--border-strong)"}`,
                background: m.selected ? "var(--gold)" : "transparent",
                display:"flex", alignItems:"center", justifyContent:"center",
              }}>
                {m.selected && (
                  <svg width="9" height="9" viewBox="0 0 9 9" fill="none"
                    stroke="#001A0A" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M1.5 4.5L4 7l4-5"/>
                  </svg>
                )}
              </span>
              <span style={{fontSize:12, color: m.selected ? "var(--gold)" : "var(--text)"}}>{m.name}</span>
              <span className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>{m.count}</span>
            </div>
          ))}
        </DrawerSection>

        {/* STYLE */}
        <DrawerSection title="Style">
          <div style={{display:"flex", gap:6, flexWrap:"wrap"}}>
            {["Long","Long/Short","Day","Swing","Mean-reversion","Momentum"].map((s) => (
              <span key={s} style={{
                padding:"4px 9px", borderRadius:3,
                border:"1px solid var(--border-strong)",
                background:"transparent",
                color:"var(--text-2)",
                fontFamily:"'Geist Mono', monospace", fontSize:10.5, cursor:"pointer",
              }}>{s}</span>
            ))}
          </div>
        </DrawerSection>

        {/* TRUST */}
        <DrawerSection title="Trust">
          <div style={{display:"flex", flexDirection:"column", gap:8}}>
            <DrawerToggle label="Verified only" subtitle="green-check strategies" on={true}/>
            <DrawerToggle label="Accepts agents (🤖 x402)" subtitle="agent-paid purchase" on={false}/>
            <DrawerToggle label="Audited only" subtitle="creator audit attestation" on={false}/>
          </div>
        </DrawerSection>

        {/* PRICE + BUYERS */}
        <DrawerSection title="Price (USDC)">
          <DrawerRange min={0} max={500} from={0} to={120}/>
        </DrawerSection>

        <DrawerSection title="Minimum buyers">
          <DrawerRange min={0} max={500} from={20} to={500} singleMin/>
        </DrawerSection>

      </div>

      {/* Footer */}
      <div style={{
        padding:"12px 16px",
        borderTop:"1px solid var(--border)",
        background:"#050505",
        display:"flex", alignItems:"center", gap:8,
      }}>
        <button style={{
          fontSize:11.5, color:"var(--text-3)", background:"transparent", border:"none",
          cursor:"pointer", textDecoration:"underline dotted", textUnderlineOffset:3,
          padding:0,
        }}>Clear all</button>
        <span className="mono" style={{marginLeft:"auto", fontSize:11, color:"var(--text-3)"}}>
          <span style={{color:"var(--gold)"}}>342</span> matches
        </span>
        <Btn variant="primary" style={{minWidth:88, justifyContent:"center"}}>Apply</Btn>
      </div>
    </div>
  </>
);

// Drawer section wrapper
const DrawerSection = ({ title, subCount, children }) => (
  <div style={{
    padding:"14px 18px",
    borderBottom:"1px solid var(--border)",
  }}>
    <div style={{
      display:"flex", alignItems:"baseline", justifyContent:"space-between",
      marginBottom:10,
    }}>
      <span style={{fontSize:12.5, fontWeight:600, color:"var(--text)"}}>{title}</span>
      {subCount && (
        <span className="mono" style={{fontSize:10, color:"var(--text-3)"}}>{subCount}</span>
      )}
    </div>
    {children}
  </div>
);

const DrawerToggle = ({ label, subtitle, on }) => (
  <div style={{
    display:"flex", alignItems:"center", gap:10,
    padding:"4px 0", cursor:"pointer",
  }}>
    <span style={{
      width:30, height:17, borderRadius:9, position:"relative", flexShrink:0,
      background: on ? "var(--gold)" : "var(--border-strong)",
      transition:"background 0.18s",
    }}>
      <span style={{
        position:"absolute", top:2, left: on ? 15 : 2,
        width:13, height:13, borderRadius:"50%", background:"#000",
        transition:"left 0.18s",
      }}/>
    </span>
    <div>
      <div style={{fontSize:12, color:"var(--text)"}}>{label}</div>
      <div className="mono" style={{fontSize:10, color:"var(--text-3)", marginTop:1}}>{subtitle}</div>
    </div>
  </div>
);

const DrawerRange = ({ min, max, from, to, singleMin = false }) => {
  const fromPct = ((from - min) / (max - min)) * 100;
  const toPct   = ((to - min)   / (max - min)) * 100;
  return (
    <div>
      <div style={{position:"relative", height:30, padding:"10px 0"}}>
        <div style={{
          position:"absolute", left:0, right:0, top:14, height:3, borderRadius:2,
          background:"var(--border-strong)",
        }}/>
        <div style={{
          position:"absolute", left:`${fromPct}%`, right:`${100 - toPct}%`,
          top:14, height:3, borderRadius:2, background:"var(--gold)",
        }}/>
        {!singleMin && (
          <div style={{
            position:"absolute", left:`${fromPct}%`, top:9,
            width:14, height:14, borderRadius:"50%",
            background:"#000", border:"2px solid var(--gold)",
            transform:"translateX(-7px)", cursor:"pointer",
          }}/>
        )}
        <div style={{
          position:"absolute", left:`${toPct}%`, top:9,
          width:14, height:14, borderRadius:"50%",
          background:"#000", border:"2px solid var(--gold)",
          transform:"translateX(-7px)", cursor:"pointer",
        }}/>
      </div>
      <div style={{
        display:"flex", justifyContent:"space-between", marginTop:4,
        fontFamily:"'Geist Mono', monospace", fontSize:11, color:"var(--text-2)",
      }}>
        <span>{singleMin ? `min ${from}` : `${from}`}</span>
        <span style={{color:"var(--text-3)"}}>{singleMin ? `unlimited` : `${to}`}</span>
      </div>
    </div>
  );
};

const Segmented = ({ options, active }) => (
  <div style={{
    display:"inline-flex", border:"1px solid var(--border-strong)", borderRadius:4,
    background:"var(--surface-elev)", padding:2,
  }}>
    {options.map((o) => {
      const isActive = o.key === active;
      return (
        <div key={o.key} style={{
          padding:"5px 12px", borderRadius:3,
          background: isActive ? "var(--gold)" : "transparent",
          color: isActive ? "#001A0A" : "var(--text-2)",
          fontSize:12, fontWeight:600, cursor:"pointer",
          fontFamily:"'Geist', sans-serif",
        }}>{o.label}</div>
      );
    })}
  </div>
);

const Chip = ({ active, children }) => (
  <span style={{
    padding:"3px 9px", borderRadius:3,
    border: active ? "1px solid var(--gold-soft)" : "1px solid var(--border-strong)",
    background: active ? "var(--gold-bg)" : "transparent",
    color: active ? "var(--gold)" : "var(--text-2)",
    fontFamily:"'Geist Mono', monospace", fontSize:10.5, letterSpacing:"0.05em",
    cursor:"pointer", userSelect:"none",
  }}>{children}</span>
);

const AssetPill = ({ a }) => {
  const tones = {
    BTC:   { fg:"#FBBF24", bg:"rgba(251,191,36,0.10)", bd:"rgba(251,191,36,0.35)" },
    ETH:   { fg:"#5FA8FF", bg:"rgba(95,168,255,0.10)", bd:"rgba(95,168,255,0.35)" },
    SOL:   { fg:"#A78BFA", bg:"rgba(167,139,250,0.10)", bd:"rgba(167,139,250,0.35)" },
    DOGE:  { fg:"#F472B6", bg:"rgba(244,114,182,0.10)", bd:"rgba(244,114,182,0.35)" },
  };
  const t = tones[a] || { fg:"var(--text-2)", bg:"var(--surface-elev)", bd:"var(--border-strong)" };
  return (
    <span style={{
      padding:"2px 6px", borderRadius:3,
      border:`1px solid ${t.bd}`, background:t.bg, color:t.fg,
      fontFamily:"'Geist Mono', monospace", fontSize:10, fontWeight:600, letterSpacing:"0.06em",
    }}>{a}</span>
  );
};

const VerifiedBadge = () => (
  <span title="Backtested + live-paper data committed on chain"
    style={{
      display:"inline-flex", alignItems:"center", gap:3,
      padding:"1px 5px", borderRadius:2,
      border:"1px solid var(--gold-soft)", background:"var(--gold-bg)",
    }}>
    <Icon name="check" size={9} color="var(--gold)" sw={2}/>
  </span>
);

const X402Badge = () => (
  <span title="Accepts agent-paid auto-purchase (x402)"
    style={{
      display:"inline-flex", alignItems:"center", gap:4,
      padding:"1px 5px", borderRadius:2,
      border:"1px solid var(--gold-soft)", background:"var(--gold-bg)",
    }}>
    <AgentIcon size={9}/>
    <span className="mono" style={{fontSize:9, color:"var(--gold)", letterSpacing:"0.14em", fontWeight:600}}>x402</span>
  </span>
);

const StrategyRow = ({ s, index }) => {
  const positive = !s.ret30.startsWith("-");
  const retColor = positive ? "var(--gold)" : "var(--danger)";
  return (
    <div style={{
      display:"grid",
      gridTemplateColumns:"56px 1.8fr 0.75fr 1.1fr 1.05fr 0.6fr 0.85fr 110px",
      alignItems:"center", gap:14,
      padding:"12px 22px",
      borderBottom:"1px solid var(--border-soft)",
      cursor:"pointer",
    }}>
      {/* gen-art thumb */}
      <div>
        <GenArt seed={s.lineageBase} size={48}/>
      </div>

      {/* name @creator + verified + x402 + sparkline */}
      <div style={{minWidth:0}}>
        <div style={{display:"flex", alignItems:"center", gap:7, flexWrap:"nowrap", whiteSpace:"nowrap"}}>
          <span className="mono" style={{fontSize:13, color:"var(--text)", fontWeight:600}}>
            {s.id}
          </span>
          <span className="mono" style={{fontSize:11, color:"var(--text-3)"}}>{s.ver}</span>
          {s.verified && <VerifiedBadge/>}
          {s.x402 && <X402Badge/>}
        </div>
        <div style={{display:"flex", alignItems:"center", gap:8, marginTop:4, whiteSpace:"nowrap"}}>
          <span className="mono" style={{fontSize:11, color:"var(--text-2)"}}>{s.creator}</span>
          <span style={{color:"var(--text-4)", fontSize:10}}>·</span>
          <span className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>{s.model}</span>
          <span style={{color:"var(--text-4)", fontSize:10}}>·</span>
          <span className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>{s.style}</span>
        </div>
      </div>

      {/* asset pills */}
      <div style={{display:"flex", gap:4, flexWrap:"wrap"}}>
        {s.assets.map((a) => <AssetPill key={a} a={a}/>)}
      </div>

      {/* 30d return + sparkline */}
      <div style={{display:"flex", alignItems:"center", gap:10, justifyContent:"flex-end"}}>
        <span className="mono" style={{
          fontSize:16, fontWeight:600, color:retColor, letterSpacing:"-0.01em",
        }}>{s.ret30}</span>
        <Sparkline seed={s.id} positive={positive}/>
      </div>

      {/* buyers humans + agents */}
      <div style={{display:"flex", alignItems:"center", gap:8}}>
        <span className="mono" style={{fontSize:13, color:"var(--text)"}}>
          {s.buyersH.toLocaleString()}
        </span>
        <span style={{color:"var(--text-4)", fontSize:10}}>·</span>
        <span style={{display:"inline-flex", alignItems:"center", gap:4,
          padding:"1px 6px", borderRadius:3,
          background:"var(--gold-bg)", border:"1px solid var(--gold-soft)",
        }}>
          <AgentIcon size={10}/>
          <span className="mono" style={{fontSize:11, color:"var(--gold)", fontWeight:600}}>{s.buyersA}</span>
        </span>
      </div>

      {/* sharpe muted */}
      <div style={{textAlign:"right"}}>
        <span className="mono" style={{fontSize:12, color:"var(--text-3)"}}>{s.sharpe}</span>
      </div>

      {/* price */}
      <div>
        {s.priceKind === "free" ? (
          <span style={{
            display:"inline-flex", alignItems:"center", gap:5,
            padding:"3px 8px", border:"1px solid var(--gold-soft)",
            background:"var(--gold-bg)", borderRadius:3,
          }}>
            <span style={{width:5, height:5, borderRadius:"50%", background:"var(--gold)"}}/>
            <span className="mono" style={{fontSize:10.5, color:"var(--gold)", letterSpacing:"0.14em", fontWeight:600}}>OPEN</span>
          </span>
        ) : (
          <span className="mono" style={{fontSize:13, color:"var(--text)"}}>
            {s.price}
          </span>
        )}
      </div>

      {/* CTA */}
      <div>
        <Btn variant="primary" dense>
          {s.priceKind === "free" ? "Run free" : "Buy"}
        </Btn>
      </div>
    </div>
  );
};

window.MarketplaceBrowse = MarketplaceBrowse;
