// Signal brand mark — a 14×14 filled `--gold` square + the "XVN" wordmark in
// Geist Mono 700 with wide tracking. Used in the EvalTopBar breadcrumb and
// anywhere the product brand needs a compact, baseline-aligned anchor.
//
// README §"Brand mark" / Task B4 step 2. The square uses the `--gold` token so
// it inherits the Signal green automatically once Phase A lands.

export function BrandMark() {
  return (
    <div className="flex items-center gap-2 leading-none">
      <span
        aria-hidden
        className="inline-block"
        style={{ width: 14, height: 14, background: "var(--gold)", borderRadius: 2 }}
      />
      <span
        className="font-mono text-text"
        style={{ fontSize: 14, fontWeight: 700, letterSpacing: "0.18em" }}
      >
        XVN
      </span>
    </div>
  );
}
