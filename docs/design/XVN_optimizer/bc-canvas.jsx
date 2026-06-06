// Root — design canvas hosting all five blockchain frames

const FRAMES = [
  { id:"marketplace-home",  label:"Marketplace · Mantle mainnet",      w:1440, h:900, Component: window.MarketplaceHome },
  { id:"lineage-detail",    label:"Lineage detail (btc-momentum)",     w:1440, h:900, Component: window.LineageDetail },
  { id:"marketplace-optin", label:"Marketplace · opt-in (no wallet)",  w:1440, h:900, Component: window.MarketplaceOptIn },
  { id:"settings-mkt",      label:"Settings · Marketplace",            w:1440, h:900, Component: window.SettingsMarketplace },
  { id:"settings-identity", label:"Settings · Identity (ERC-8004)",    w:1440, h:900, Component: window.SettingsIdentity },
];

const App = () => (
  <DesignCanvas>
    <DCSection id="blockchain-pages" title="Marketplace & ERC-8004 identity · Signal theme">
      {FRAMES.map(f => (
        <DCArtboard key={f.id} id={f.id} label={f.label} width={f.w} height={f.h}>
          <f.Component/>
        </DCArtboard>
      ))}
    </DCSection>
  </DesignCanvas>
);

ReactDOM.createRoot(document.getElementById("root")).render(<App/>);
