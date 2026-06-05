export function EmptyPanel({
  title,
  phase,
  hint,
}: {
  title: string;
  phase: 2 | 3 | 4;
  hint: string;
}) {
  return (
    <section className="rounded-md border border-dashed border-border-strong bg-surface-card p-5">
      <div className="flex items-center justify-between">
        <h2 className="m-0 text-[15px] font-semibold tracking-tight">{title}</h2>
        <span className="rounded border border-border px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wide text-text-3">
          Phase {phase}
        </span>
      </div>
      <p className="mt-2 text-[12px] text-text-3">{hint}</p>
    </section>
  );
}
