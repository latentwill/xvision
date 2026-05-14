type Props = { onPick: (start: string, end: string) => void };

export function RegimeRangePresets({ onPick }: Props) {
  const today = new Date();
  const fmt = (d: Date) => d.toISOString().slice(0, 10);

  function back(days: number) {
    const d = new Date(today);
    d.setDate(d.getDate() - days);
    onPick(fmt(d), fmt(today));
  }

  function lastYear() {
    const start = new Date(today.getFullYear() - 1, 0, 1);
    const end = new Date(today.getFullYear() - 1, 11, 31);
    onPick(fmt(start), fmt(end));
  }

  function ytd() {
    const start = new Date(today.getFullYear(), 0, 1);
    onPick(fmt(start), fmt(today));
  }

  return (
    <div className="flex gap-1.5 text-[12px]">
      <button type="button" onClick={lastYear} className="px-2 py-1 border border-border rounded">
        Last year
      </button>
      <button type="button" onClick={ytd} className="px-2 py-1 border border-border rounded">
        YTD
      </button>
      <button type="button" onClick={() => back(90)} className="px-2 py-1 border border-border rounded">
        Last 90 days
      </button>
      <button type="button" onClick={() => back(30)} className="px-2 py-1 border border-border rounded">
        Last 30 days
      </button>
    </div>
  );
}
