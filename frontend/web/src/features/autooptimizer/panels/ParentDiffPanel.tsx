import { useBlob, type StrategyBlob } from "../api";

type Row = { key: string; before: unknown; after: unknown; changed: boolean };

function flatten(obj: unknown, prefix = ""): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  if (obj && typeof obj === "object" && !Array.isArray(obj)) {
    for (const [k, v] of Object.entries(obj as Record<string, unknown>)) {
      const key = prefix ? `${prefix}.${k}` : k;
      if (v && typeof v === "object" && !Array.isArray(v)) Object.assign(out, flatten(v, key));
      else out[key] = v;
    }
  }
  return out;
}

function diffRows(parent: StrategyBlob | undefined, child: StrategyBlob | undefined): Row[] {
  const p = flatten(parent ?? {});
  const c = flatten(child ?? {});
  const keys = Array.from(new Set([...Object.keys(p), ...Object.keys(c)])).sort();
  return keys.map((key) => ({
    key,
    before: p[key],
    after: c[key],
    changed: JSON.stringify(p[key]) !== JSON.stringify(c[key]),
  }));
}

function cell(v: unknown): string {
  if (v === undefined) return "—";
  return typeof v === "string" ? v : JSON.stringify(v);
}

export function ParentDiffPanel({
  childHash,
  parentHash,
}: {
  childHash: string;
  parentHash?: string | null;
}) {
  const child = useBlob(childHash);
  const parent = useBlob(parentHash ?? undefined);
  const rows = diffRows(parent.data, child.data);
  const changed = rows.filter((r) => r.changed);
  const loading = child.isLoading || (!!parentHash && parent.isLoading);

  return (
    <section className="rounded-md border border-border bg-surface-card p-5 text-left">
      <h2 className="m-0 text-[15px] font-semibold tracking-tight">What this experiment changed</h2>
      <p className="m-0 mt-0.5 text-[12px] text-text-3">
        parent → experiment · {changed.length} field{changed.length === 1 ? "" : "s"} changed
      </p>
      {loading ? (
        <p className="m-0 mt-3 text-[12px] text-text-3">Loading diff…</p>
      ) : !parentHash ? (
        <p className="m-0 mt-3 text-[12px] text-text-3">Root experiment — no parent to diff against.</p>
      ) : changed.length === 0 ? (
        <p className="m-0 mt-3 text-[12px] text-text-3">No field-level differences from the parent.</p>
      ) : (
        <table className="mt-3 w-full table-fixed border-collapse text-left font-mono text-[11.5px]">
          <thead>
            <tr className="text-left text-text-3">
              <th className="w-[120px] py-1.5 pr-3 font-medium">Field</th>
              <th className="py-1.5 pr-3 font-medium">− before</th>
              <th className="py-1.5 font-medium">+ after</th>
            </tr>
          </thead>
          <tbody>
            {changed.map((r) => (
              <tr key={r.key} className="border-t border-border-soft align-top">
                <td className="py-1.5 pr-3 text-text-2 whitespace-pre-wrap [overflow-wrap:anywhere]">
                  {r.key}
                </td>
                <td className="py-1.5 pr-3 text-danger whitespace-pre-wrap [overflow-wrap:anywhere]">
                  {cell(r.before)}
                </td>
                <td className="py-1.5 text-gold whitespace-pre-wrap [overflow-wrap:anywhere]">
                  {cell(r.after)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
