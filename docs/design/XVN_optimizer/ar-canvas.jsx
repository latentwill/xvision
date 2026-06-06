// Autoresearch · canvas (entry point)
const AR_FRAMES = [
  { id:"ar-home",     label:"Optimizer · home · /research",
    w:1440, h:900,  C: window.ARHome },
  { id:"ar-cycle",    label:"Cycle detail · /research/cycle/cyc-01N8R2K9 · btc-momentum",
    w:1440, h:1640, C: window.ARCycle },
  { id:"ar-variant",  label:"Variant inspector · /research/variant/v3.1.g · kept survivor",
    w:1440, h:1400, C: window.ARVariant },
  { id:"ar-settings", label:"Settings · Optimizer · /settings/optimizer",
    w:1440, h:1200, C: window.ARSettings },
];

const ARApp = () => (
  <DesignCanvas>
    <DCSection
      id="autoresearch"
      title="Optimizer · overnight loop · experiment → eval → gate → keep"
    >
      {AR_FRAMES.filter(f => f.C).map(f => (
        <DCArtboard key={f.id} id={f.id} label={f.label} width={f.w} height={f.h}>
          <f.C/>
        </DCArtboard>
      ))}
    </DCSection>
  </DesignCanvas>
);

ReactDOM.createRoot(document.getElementById("root")).render(<ARApp/>);
