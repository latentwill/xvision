type LayerItem = {
  key: string;
  label: string;
  on: boolean;
};

type LayerGroup = {
  title: string;
  items: LayerItem[];
};

type Props = {
  groups: LayerGroup[];
  onToggle: (key: string) => void;
};

export function LayerPanel({ groups, onToggle }: Props) {
  return (
    <div className="space-y-3 text-[12px] text-text">
      {groups.map((group) => (
        <fieldset key={group.title} className="border-0 p-0 m-0">
          <legend className="text-[11px] font-medium uppercase tracking-wide text-text-3 mb-1.5">
            {group.title}
          </legend>
          <div className="space-y-1">
            {group.items.map((item) => (
              <label
                key={item.key}
                className="flex items-center gap-2 cursor-pointer hover:text-text-2 transition-colors"
              >
                <input
                  type="checkbox"
                  checked={item.on}
                  onChange={() => onToggle(item.key)}
                  className="accent-current"
                />
                <span>{item.label}</span>
              </label>
            ))}
          </div>
        </fieldset>
      ))}
    </div>
  );
}
