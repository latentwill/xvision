type Props = { onPick: (start: string, end: string) => void };

function formatLocalDate(d: Date) {
  const yyyy = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  return `${yyyy}-${mm}-${dd}`;
}

export function RegimeRangePresets({ onPick }: Props) {
  function back(days: number) {
    const today = new Date();
    const d = new Date(today);
    d.setDate(d.getDate() - days);
    onPick(formatLocalDate(d), formatLocalDate(today));
  }

  function lastYear() {
    const today = new Date();
    const start = new Date(today.getFullYear() - 1, 0, 1);
    const end = new Date(today.getFullYear() - 1, 11, 31);
    onPick(formatLocalDate(start), formatLocalDate(end));
  }

  function ytd() {
    const today = new Date();
    const start = new Date(today.getFullYear(), 0, 1);
    onPick(formatLocalDate(start), formatLocalDate(today));
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
