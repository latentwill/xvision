// Root canvas — vibetrader-facing marketplace + lineage identity
//
// Three frames showing the shift:
//   1. /marketplace — browse, replaces the operator 2×2 panels
//   2. /marketplace/lineage/btc-momentum-v3 — the viral identity page,
//      drawer closed (the buyer view).
//   3. Same page, drawer expanded — proving the auditor surface is still
//      one click away.

const FRAMES = [
  { id:"mp-browse",        label:"Marketplace · vibetrader browse",       w:1440, h:900,  C: window.MarketplaceBrowse },
  { id:"creator-profile",  label:"Creator profile · /creator/ed",         w:1440, h:1500, C: window.CreatorProfile },
  { id:"lineage-identity", label:"Lineage identity · /lineage/btc-momentum-v3",  w:1440, h:1500, C: () => <window.LineageIdentity drawerOpen={false}/> },
  { id:"lineage-receipts", label:"Lineage identity · on-chain receipts drawer expanded", w:1440, h:2320, C: () => <window.LineageIdentity drawerOpen={true}/> },
  { id:"purchase-receipt", label:"Purchase receipt · post-buy · /receipts/0xa83e…", w:1440, h:900,  C: window.PurchaseReceipt },
  { id:"shareable-card",   label:"Shareable card · 1200 × 630 OG · the screenshot moment", w:1200, h:630,  C: window.ShareableCardFrame },
];

const App = () => (
  <DesignCanvas>
    <DCSection
      id="marketplace-shift"
      title="Marketplace + identity · vibetrader shift"
    >
      {FRAMES.map(f => (
        <DCArtboard key={f.id} id={f.id} label={f.label} width={f.w} height={f.h}>
          <f.C/>
        </DCArtboard>
      ))}
    </DCSection>
  </DesignCanvas>
);

ReactDOM.createRoot(document.getElementById("root")).render(<App/>);
