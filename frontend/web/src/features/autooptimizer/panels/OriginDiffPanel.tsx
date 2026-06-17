import { useOriginDiff } from "../api";

interface Props {
  hash: string;
}

function DiffRow({ label, before, after }: { label: string; before: string; after: string }) {
  return (
    <div className="space-y-1">
      <div className="text-[11px] font-medium text-text-2 uppercase tracking-wider">{label}</div>
      <div className="grid grid-cols-2 gap-2">
        <div className="min-w-0 rounded border border-border bg-surface-muted p-2 font-mono text-[11px] text-text-3 line-through whitespace-pre-wrap [overflow-wrap:anywhere]">
          {before || <span className="italic text-text-4">empty</span>}
        </div>
        <div className="min-w-0 rounded border border-border bg-surface-muted p-2 font-mono text-[11px] whitespace-pre-wrap [overflow-wrap:anywhere]">
          {after || <span className="italic text-text-4">empty</span>}
        </div>
      </div>
    </div>
  );
}

export function OriginDiffPanel({ hash }: Props) {
  const { data, isLoading, isError } = useOriginDiff(hash);

  if (isLoading) return <p className="text-[12px] text-text-3">Loading origin diff…</p>;
  if (isError || !data) return <p className="text-[12px] text-danger">Could not load origin diff.</p>;

  const { origin_hash, diff } = data;
  const hasChanges =
    diff.prose.length > 0 ||
    diff.params.length > 0 ||
    diff.tools.added.length > 0 ||
    diff.tools.removed.length > 0 ||
    diff.filter.length > 0;

  return (
    <div className="space-y-3">
      <div className="font-mono text-[11px] text-text-3">
        origin: {origin_hash.slice(0, 16)}
      </div>
      {!hasChanges ? (
        <p className="text-[12px] text-text-3">No changes from originating strategy.</p>
      ) : (
        <div className="space-y-3">
          {diff.prose.map((p, i) => (
            <DiffRow key={i} label={`prose · ${p.agent_role}`} before={p.before} after={p.after} />
          ))}
          {diff.params.map((p, i) => (
            <DiffRow key={i} label={`param · ${p.key}`} before={String(p.before)} after={String(p.after)} />
          ))}
          {diff.tools.added.map((t, i) => (
            <DiffRow key={`add-${i}`} label="tool added" before="" after={t} />
          ))}
          {diff.tools.removed.map((t, i) => (
            <DiffRow key={`rm-${i}`} label="tool removed" before={t} after="" />
          ))}
          {diff.filter.map((f, i) => (
            <DiffRow key={i} label={`filter · ${f.path}`} before={String(f.before)} after={String(f.after)} />
          ))}
        </div>
      )}
    </div>
  );
}
