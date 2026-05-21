const MAX_VISIBLE_ROWS = 200;

type Column = {
  key: string;
  header: string;
  align?: "left" | "right";
};

type Row = Record<string, string | number>;

type Props = {
  columns: Column[];
  rows: Row[];
};

export function DataTable({ columns, rows }: Props) {
  const overflow = rows.length > MAX_VISIBLE_ROWS;
  const visibleRows = overflow ? rows.slice(-MAX_VISIBLE_ROWS) : rows;
  const hiddenCount = rows.length - visibleRows.length;

  return (
    <div className="overflow-x-auto w-full">
      {overflow && (
        <p className="text-[11px] text-text-3 px-3 pt-2">
          … +{hiddenCount} more rows (showing last {MAX_VISIBLE_ROWS})
        </p>
      )}
      <table className="w-full border-collapse text-[12px]">
        <thead>
          <tr className="border-b border-border">
            {columns.map((col) => (
              <th
                key={col.key}
                className={`px-3 py-2 text-[11px] font-medium uppercase tracking-wide text-text-3 whitespace-nowrap ${
                  col.align === "right" ? "text-right" : "text-left"
                }`}
              >
                {col.header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {visibleRows.map((row, rowIdx) => (
            <tr
              key={rowIdx}
              className={
                rowIdx % 2 === 0
                  ? "bg-transparent"
                  : "bg-surface-elev/40"
              }
            >
              {columns.map((col) => (
                <td
                  key={col.key}
                  className={`px-3 py-1.5 text-text-2 whitespace-nowrap ${
                    col.align === "right" ? "text-right font-mono" : "text-left"
                  }`}
                >
                  {row[col.key] ?? ""}
                </td>
              ))}
            </tr>
          ))}
          {visibleRows.length === 0 && (
            <tr>
              <td
                colSpan={columns.length}
                className="px-3 py-4 text-center text-text-3 text-[12px]"
              >
                No data
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}
