import type { RouteReadiness } from "@/api/strategies";

export function RouteReadinessPanel({
  message,
  readiness,
  error,
}: {
  message: string | null;
  readiness?: RouteReadiness | null;
  error?: string | null;
}) {
  const tone = error || message ? "border-warn/30 bg-warn/10 text-warn" : "border-gold/30 bg-gold/10 text-text";

  return (
    <section className={`rounded border px-3 py-3 ${tone}`}>
      <div className="mb-1 text-[12px] uppercase tracking-wide">Readiness</div>
      {error ? (
        <div className="text-[13px] leading-relaxed">{error}</div>
      ) : message ? (
        <div className="text-[13px] leading-relaxed">{message}</div>
      ) : (
        <div className="text-[13px] leading-relaxed">
          Route is ready once the remaining strategy validation checks pass.
        </div>
      )}
      {readiness ? (
        <div className="mt-2 font-mono text-[11px] text-text-3">
          routed={String(readiness.routed)} · context={readiness.context_fields.join(", ") || "none"}
        </div>
      ) : null}
    </section>
  );
}
