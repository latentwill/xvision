// xvn — Mobile ListCard anatomy
const MListAnatomy = () => {
  return (
    <div style={{
      background: "var(--bg)", padding: "44px 50px 50px",
      width: 1440, minHeight: 900, color: "var(--text)",
      fontFamily: "Inter, sans-serif", position: "relative",
    }}>
      <div style={{marginBottom: 28}}>
        <div style={{fontSize: 11, color: "var(--text-3)", letterSpacing: "0.12em", textTransform: "uppercase", marginBottom: 6}}>
          Component · mobile
        </div>
        <h1 className="serif" style={{fontSize: 48, margin: 0, letterSpacing: "-0.02em"}}>
          <span className="serif-i">List</span>
          <span style={{color: "var(--text-2)"}}>Card</span>
          <span style={{fontSize: 28, color: "var(--text-3)", marginLeft: 14}}>· mobile</span>
        </h1>
        <div style={{color: "var(--text-2)", fontSize: 14, marginTop: 6, maxWidth: 700}}>
          Same contract — search, filters, sort by added. Different shape: no table,
          card-style rows, search-always-visible, filters in a bottom sheet.
        </div>
      </div>

      <div style={{display: "grid", gridTemplateColumns: "420px 1fr", gap: 56, alignItems: "start"}}>
        {/* Phone */}
        <div style={{position: "relative"}}>
          <IOSDevice width={402} height={874} dark={true} keyboard={false}>
            <MobileStrategiesList />
          </IOSDevice>
        </div>

        {/* Spec column */}
        <div style={{display: "flex", flexDirection: "column", gap: 22, paddingTop: 20}}>
          <SpecBlock
            num="1"
            title="Sticky header"
            body={<>Title in <span className="serif-i">Cormorant</span> + count pill in mono. Right-side icon button is the page CTA (here: <code>+</code> new strategy).</>}
          />
          <SpecBlock
            num="2"
            title="Search — always visible"
            body="Full-width pill input. No collapse-to-icon affordance — on mobile the search is the most-used path, so it earns its space."
          />
          <SpecBlock
            num="3"
            title="One control row"
            body={<>
              <b>Filter</b> pill (with badge when filters are active) opens the full sheet.
              <br/>
              <b>Sort</b> pill shows the current key and opens a sort-focused sheet on tap.
            </>}
          />
          <SpecBlock
            num="4"
            title="Active filter chips"
            body="Inline below the controls when non-default. Tap any chip to clear that one; 'Clear' wipes all."
          />
          <SpecBlock
            num="5"
            title="Card-style rows"
            body={<>
              No tables on mobile. Each row is a tappable card:
              <br/>· left column: title (mono) + badge, subtitle, meta line
              <br/>· right column: hero metric in serif + sub label in mono
            </>}
          />
          <SpecBlock
            num="6"
            title="Bottom sheet"
            body="Tap Filter (or Sort) to open. Every filter is a single-select chip group; sort options are a radio list. Apply button shows the live result count so users know what they'll get."
          />
        </div>
      </div>
    </div>
  );
};

const SpecBlock = ({ num, title, body }) => (
  <div style={{display: "flex", gap: 14}}>
    <div style={{
      width: 28, height: 28, flexShrink: 0,
      borderRadius: "50%",
      background: "rgba(0, 230, 118, 0.12)",
      border: "1px solid rgba(0, 230, 118, 0.4)",
      display: "flex", alignItems: "center", justifyContent: "center",
      color: "var(--gold)", fontFamily: "'Geist Mono', monospace",
      fontSize: 12, fontWeight: 500,
    }}>{num}</div>
    <div style={{flex: 1, minWidth: 0}}>
      <div className="serif" style={{fontSize: 19, color: "var(--text)", marginBottom: 4, letterSpacing: "-0.01em"}}>
        {title}
      </div>
      <div style={{fontSize: 13, color: "var(--text-2)", lineHeight: 1.55, maxWidth: 540}}>
        {body}
      </div>
    </div>
  </div>
);

window.MListAnatomy = MListAnatomy;
