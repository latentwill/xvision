export function ChatComposer({
  value,
  placeholder,
  onChange,
  onSubmit,
  disabled,
  onOpenActions,
}: {
  value: string;
  placeholder: string;
  onChange: (s: string) => void;
  onSubmit: () => void;
  disabled: boolean;
  onOpenActions?: () => void;
}) {
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit();
      }}
      className="border-t border-border-soft px-3 py-2.5 flex gap-2 bg-surface-2/30"
    >
      {onOpenActions && (
        <button
          type="button"
          onClick={onOpenActions}
          className="w-8 h-8 rounded-full border border-border-soft bg-surface-2/60 text-text-2 hover:text-text flex items-center justify-center disabled:opacity-50"
          disabled={disabled}
          aria-label="Open all functions"
        >
          +
        </button>
      )}
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={disabled}
        placeholder={placeholder}
        className="flex-1 bg-transparent border border-border-soft rounded-md px-2.5 py-1.5 text-[13px] placeholder:text-text-3 focus:outline-none focus:ring-1 focus:ring-text-2"
      />
      <button
        type="submit"
        disabled={disabled || !value.trim()}
        className="px-2.5 py-1.5 rounded-md text-[12px] border border-border-soft bg-surface-2/60 hover:bg-surface-2 disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {disabled ? "..." : "Send"}
      </button>
    </form>
  );
}
