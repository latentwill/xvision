// Frame 5 — /settings/marketplace/identity · Identity (ERC-8004)
// Three identity cards (operator + 2 attesters), manifest preview, "what this signs" list.

const IDENTITIES = [
  {
    role:"operator",
    name:"Xvision · operator",
    token:"#0001",
    blurb:"Platform agent #0 · the xvn-the-operator identity that signs eval attestations and anchors lineages on Mantle.",
    cid:"bafybeib4xj…q2y7l",
    owner:"0xa83e…f12d4",
    mintTx:"0x4f8a…ee01",
    born:"2026-05-13 04:11Z",
    capabilities:[
      "ERC-8004 agent registration",
      "Lineage NFT mint (Identity Registry)",
      "Merkle root post (Reputation Registry)",
      "Anchor SessionCommitment at session start",
    ],
    activity:{ kind:"Operator", endorse:null, mints:9, anchors:6, last:"14 min ago" },
    accent:"gold",
    primary:true,
  },
  {
    role:"attester",
    name:"regime-verifier",
    token:"#0007",
    blurb:"Reads each committed Finding and compares the claim's regime_affinity against the actual regime tags in the variant's trace tape.",
    cid:"bafybeic7th…a2hu9",
    owner:"0xb12f…aa78c",
    mintTx:"0x9183…07c1",
    born:"2026-05-13 04:18Z",
    capabilities:[
      "Read CycleSeal feed",
      "Post ValidationReceipt (Endorse · Question · Reject)",
      "Pin rationale to IPFS",
    ],
    activity:{ kind:"Attester", endorse:27, question:4, reject:0, last:"2 min ago" },
    accent:"info",
  },
  {
    role:"attester",
    name:"diversity-check",
    token:"#0008",
    blurb:"Computes embedding distance from existing siblings in the lineage. Endorses if distance ≥ 0.18 threshold; questions otherwise.",
    cid:"bafybeid9p2…ke3w1",
    owner:"0xc7a4…bd221",
    mintTx:"0xa710…45fe",
    born:"2026-05-13 04:22Z",
    capabilities:[
      "Read CycleSeal feed",
      "Compute embedding diversity",
      "Post ValidationReceipt",
      "Pin rationale to IPFS",
    ],
    activity:{ kind:"Attester", endorse:21, question:6, reject:0, last:"9 min ago" },
    accent:"violet",
  },
];

const SettingsIdentity = () => (
  <Frame>
    <SideNav active="settings"/>
    <main style={{display:"flex", flexDirection:"column", minWidth:0, overflow:"hidden"}}>
      <TopStatus breadcrumb={[
        { text:"SETTINGS" },
        { text:"marketplace", mono:false },
        { text:"identity", mono:true },
      ]}/>
      <div style={{flex:1, minHeight:0, display:"flex", overflow:"hidden"}}>
        <SettingsSidebar active="identity"/>
        <div style={{flex:1, padding:"26px 32px 22px", overflow:"auto", display:"flex", flexDirection:"column", gap:18}}>
          {/* Page header */}
          <div style={{display:"flex", justifyContent:"space-between", alignItems:"flex-end"}}>
            <div>
              <h1 style={{margin:0, fontSize:28, fontWeight:600, letterSpacing:"-0.03em", lineHeight:1.1}}>
                Identity <span style={{color:"var(--text-3)", fontWeight:500}}>(ERC-8004)</span>
              </h1>
              <div style={{marginTop:6, fontSize:13, color:"var(--text-2)"}}>
                ERC-721 agent NFTs that sign your on-chain activity · 3 minted · all on Mantle mainnet
              </div>
            </div>
            <div style={{display:"flex", gap:8}}>
              <Btn variant="ghost" icon="ext">View IdentityRegistry</Btn>
              <Btn variant="primary" icon="plus">Mint new identity</Btn>
            </div>
          </div>

          {/* Identity cards grid: operator across top, attesters in 2 cols below */}
          <IdentityCard id={IDENTITIES[0]}/>
          <div style={{display:"grid", gridTemplateColumns:"1fr 1fr", gap:16}}>
            <IdentityCard id={IDENTITIES[1]}/>
            <IdentityCard id={IDENTITIES[2]}/>
          </div>

          {/* Manifest preview + What this signs */}
          <div style={{display:"grid", gridTemplateColumns:"1fr 380px", gap:16}}>
            <ManifestJsonCard identity={IDENTITIES[0]}/>
            <WhatThisSignsCard/>
          </div>
        </div>
      </div>
    </main>
  </Frame>
);

// === Identity card ===
const IdentityCard = ({ id }) => {
  const accentMap = {
    gold:   { ring:"var(--gold)",   bg:"var(--gold-bg)",          chip:"var(--gold)",   fg:"var(--gold)" },
    info:   { ring:"var(--info)",   bg:"rgba(95,168,255,0.08)",   chip:"var(--info)",   fg:"var(--info)" },
    violet: { ring:"#A78BFA",       bg:"rgba(167,139,250,0.08)",  chip:"#A78BFA",       fg:"#A78BFA" },
  };
  const a = accentMap[id.accent];
  return (
    <div style={{
      border:`1px solid ${id.primary ? a.ring : "var(--border)"}`,
      borderRadius:6, overflow:"hidden", background:"transparent",
    }}>
      <div style={{
        display:"grid", gridTemplateColumns:"60px 1fr 220px",
        gap:18, padding:"18px 20px",
        borderBottom:"1px solid var(--border-soft)",
        alignItems:"start",
      }}>
        {/* NFT badge */}
        <div style={{
          width:60, height:60, borderRadius:6,
          border:`1px solid ${a.ring}`, background:a.bg,
          display:"flex", flexDirection:"column", alignItems:"center", justifyContent:"center",
          position:"relative", overflow:"hidden",
        }}>
          <Icon name={id.role === "operator" ? "diamond" : "shield"} size={20} color={a.fg}/>
          <div className="mono" style={{fontSize:9, color:a.fg, marginTop:2, letterSpacing:"0.14em"}}>{id.token}</div>
        </div>

        <div>
          <div style={{display:"flex", alignItems:"center", gap:10, flexWrap:"wrap"}}>
            <h2 style={{margin:0, fontSize:17, fontWeight:600, letterSpacing:"-0.02em"}}>{id.name}</h2>
            <span style={{
              padding:"2px 8px",
              border:`1px solid ${a.ring}`, background:a.bg, color:a.fg,
              borderRadius:3, fontFamily:"'Geist Mono', monospace", fontSize:10,
              letterSpacing:"0.16em", fontWeight:600,
            }}>{id.role.toUpperCase()} · NFT {id.token}</span>
            <StatusPill tone="gold" pulse>ACTIVE</StatusPill>
          </div>
          <div style={{fontSize:12.5, color:"var(--text-2)", marginTop:8, lineHeight:1.5, maxWidth:680}}>
            {id.blurb}
          </div>
          <div className="mono" style={{
            display:"flex", flexWrap:"wrap", gap:"6px 16px", marginTop:10, fontSize:11,
          }}>
            <span style={{color:"var(--text-3)"}}>owner <span style={{color:"var(--text)"}}>{id.owner}</span></span>
            <span style={{color:"var(--text-3)"}}>mint <span style={{color:"var(--info)"}}>{id.mintTx}</span></span>
            <span style={{color:"var(--text-3)"}}>born <span style={{color:"var(--text-2)"}}>{id.born}</span></span>
          </div>
        </div>

        {/* Activity counters */}
        <div style={{display:"flex", gap:18, justifyContent:"flex-end"}}>
          {id.role === "operator" ? (
            <>
              <CounterStat label="MINTS"    value={id.activity.mints}   color="var(--gold)"/>
              <CounterStat label="ANCHORS"  value={id.activity.anchors} color="var(--info)"/>
              <CounterStat label="LAST"     value={id.activity.last}    color="var(--text-2)" mono/>
            </>
          ) : (
            <>
              <CounterStat label="ENDORSE"  value={id.activity.endorse}  color="var(--gold)"/>
              <CounterStat label="QUESTION" value={id.activity.question} color="var(--warn)"/>
              <CounterStat label="REJECT"   value={id.activity.reject}   color="var(--danger)"/>
            </>
          )}
        </div>
      </div>

      {/* agentURI strip */}
      <div style={{
        padding:"10px 20px", display:"flex", alignItems:"center", gap:12,
        borderBottom:"1px solid var(--border-soft)",
        background:"var(--surface-elev)",
      }}>
        <span className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em", color:"var(--text-3)"}}>agentURI</span>
        <span className="mono" style={{
          flex:1, fontSize:11.5, color:"var(--info)", textDecoration:"underline dotted",
          textUnderlineOffset:2, wordBreak:"break-all",
        }}>ipfs://{id.cid}</span>
        <Btn variant="ghost" dense icon="copy">Copy</Btn>
        <Btn variant="ghost" dense icon="ext">IPFS</Btn>
        <Btn variant="ghost" dense icon="ext">Mantlescan</Btn>
      </div>

      {/* Capabilities row */}
      <div style={{
        padding:"12px 20px 14px",
        display:"flex", alignItems:"center", gap:18, flexWrap:"wrap",
      }}>
        <span className="ulabel" style={{fontSize:9.5, letterSpacing:"0.18em"}}>CAPABILITIES</span>
        {id.capabilities.map((c) => (
          <span key={c} style={{
            display:"inline-flex", alignItems:"center", gap:6,
            fontSize:11.5, color:"var(--text-2)",
          }}>
            <span style={{width:5, height:5, borderRadius:"50%", background:a.chip}}/>
            {c}
          </span>
        ))}
      </div>
    </div>
  );
};

const CounterStat = ({ label, value, color = "var(--text)", mono = false }) => (
  <div style={{textAlign:"right", minWidth:70}}>
    <div className="ulabel" style={{fontSize:9, letterSpacing:"0.16em"}}>{label}</div>
    <div className={mono ? "mono" : ""} style={{
      fontSize: mono ? 12 : 20, fontWeight:600, marginTop:3,
      letterSpacing: mono ? "0.01em" : "-0.02em",
      color, fontVariantNumeric:"tabular-nums",
    }}>{value}</div>
  </div>
);

// === Manifest JSON card (operator manifest) ===
const ManifestJsonCard = ({ identity }) => {
  const json = `{
  "schema": "https://xvision.dev/schemas/platform-agent.v1.json",
  "name": "Xvision",
  "description": "Marketplace and identity layer for AI trading agents.",
  "endpoints": {
    "marketplace_contract":      "0x…Marketplace",
    "listing_registry_contract": "0x…ListingRegistry",
    "license_token_contract":    "0x…LicenseToken",
    "eval_attestation_contract": "0x…EvalAttestationRegistry",
    "x402_buy_endpoint":         "https://api.xvn.dev/x402/listings/{id}/buy",
    "listings_browse":           "https://api.xvn.dev/listings",
    "marketplace_dapp":          "https://app.xvn.dev"
  },
  "supported_protocols": ["erc-8004", "erc-1155", "x402", "eip-3009"],
  "owner_multisig":               "0x…2of3multisig",
  "discovery_canonical_chain":    { "chainId": 5000, "name": "Mantle" }
}`;
  return (
    <Card
      title="Manifest preview"
      sub={`agentURI · pinned to IPFS · CID ${identity.cid}`}
      right={
        <div style={{display:"flex", gap:6}}>
          <Btn variant="ghost" dense icon="paste">Copy JSON</Btn>
          <Btn variant="ghost" dense icon="ext">View raw</Btn>
        </div>
      }
    >
      <pre style={{
        margin:0, padding:"14px 18px",
        fontFamily:"'Geist Mono', monospace", fontSize:11.5, lineHeight:1.6,
        color:"var(--text-2)", background:"transparent",
        whiteSpace:"pre", overflow:"auto",
      }}>
        {syntax(json)}
      </pre>
    </Card>
  );
};

// crude JSON syntax highlighter — keys in text, strings in gold, brackets in text-3
const syntax = (s) => {
  const parts = [];
  let buf = "";
  let inStr = false;
  let strBuf = "";
  let isKey = false;
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (c === '"' && !inStr) {
      if (buf) parts.push(<span key={parts.length} style={{color:"var(--text)"}}>{buf}</span>);
      buf = "";
      inStr = true;
      strBuf = '"';
    } else if (c === '"' && inStr) {
      strBuf += '"';
      // is this a key? look ahead for ':'
      let j = i + 1;
      while (j < s.length && (s[j] === " " || s[j] === "\t")) j++;
      isKey = s[j] === ":";
      parts.push(<span key={parts.length} style={{color: isKey ? "var(--text)" : "var(--gold)"}}>{strBuf}</span>);
      strBuf = "";
      inStr = false;
    } else if (inStr) {
      strBuf += c;
    } else if (c === "{" || c === "}" || c === "[" || c === "]" || c === "," || c === ":") {
      if (buf) parts.push(<span key={parts.length} style={{color:"var(--text)"}}>{buf}</span>);
      buf = "";
      parts.push(<span key={parts.length} style={{color:"var(--text-3)"}}>{c}</span>);
    } else {
      buf += c;
    }
  }
  if (buf) parts.push(<span key={parts.length} style={{color:"var(--text)"}}>{buf}</span>);
  return parts;
};

// === "What this signs" list ===
const WhatThisSignsCard = () => {
  const rows = [
    { icon:"check", label:"Autoresearch CycleSeal artifacts",  state:"active", note:"every committed bundle · ed25519" },
    { icon:"check", label:"Lineage NFT mints (Identity Registry)", state:"active", note:"per lineage · ERC-721" },
    { icon:"check", label:"Counterfactual-chain Merkle roots", state:"active", note:"Reputation Registry · snapshot or final" },
    { icon:"check", label:"ValidationReceipts (attesters)",    state:"active", note:"per attester · per bundle" },
    { icon:"radio", label:"x402 license purchases",            state:"future", note:"v2 · marketplace plugin defers" },
    { icon:"radio", label:"Pay-per-fire micropayments",        state:"future", note:"v2 · streaming x402" },
  ];
  return (
    <Card title="What this signs" sub="surfaces gated by your operator identity">
      <div style={{padding:"4px 0"}}>
        {rows.map((r, i) => (
          <div key={i} style={{
            display:"grid", gridTemplateColumns:"22px 1fr",
            gap:10, padding:"10px 16px",
            borderBottom: i < rows.length-1 ? "1px solid var(--border-soft)" : "none",
            opacity: r.state === "future" ? 0.55 : 1,
          }}>
            <Icon name={r.icon} size={14} color={r.state === "active" ? "var(--gold)" : "var(--text-3)"}/>
            <div>
              <div style={{fontSize:12.5, color: r.state === "active" ? "var(--text)" : "var(--text-2)"}}>
                {r.label}
              </div>
              <div className="mono" style={{fontSize:10.5, color:"var(--text-3)", marginTop:3}}>
                {r.note}
              </div>
            </div>
          </div>
        ))}
      </div>
    </Card>
  );
};

window.SettingsIdentity = SettingsIdentity;
