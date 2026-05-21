type LegendItem = {
  label: string;
  color: string;
  dashed?: boolean;
};

type Props = {
  items: LegendItem[];
};

export function Legend({ items }: Props) {
  if (items.length === 0) return null;

  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
      {items.map((item) => (
        <span key={item.label} className="flex items-center gap-1">
          {/* Color swatch */}
          <span
            className="inline-block w-5 h-0.5 shrink-0 rounded-full"
            style={
              item.dashed
                ? {
                    backgroundImage: `repeating-linear-gradient(90deg, ${item.color} 0 4px, transparent 4px 8px)`,
                  }
                : { backgroundColor: item.color }
            }
            aria-hidden
          />
          <span className="text-[11px] text-text-3">{item.label}</span>
        </span>
      ))}
    </div>
  );
}
