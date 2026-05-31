import { Icon } from "@/components/primitives/Icon";

export function ChatComposer({
  value,
  placeholder,
  onChange,
  onSubmit,
  disabled,
  busy = false,
  onCancel,
  onOpenActions,
}: {
  value: string;
  placeholder: string;
  onChange: (s: string) => void;
  onSubmit: () => void;
  disabled: boolean;
  busy?: boolean;
  onCancel?: () => void;
  onOpenActions?: () => void;
}) {
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        if (busy) {
          onCancel?.();
          return;
        }
        if (!value.trim()) return;
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
        type={busy && onCancel ? "button" : "submit"}
        onClick={busy && onCancel ? onCancel : undefined}
        disabled={disabled || (!busy && !value.trim())}
        className="h-8 w-8 shrink-0 rounded-sm border border-border-soft bg-surface-2/60 text-text-2 hover:bg-surface-2 hover:text-text disabled:opacity-50 disabled:cursor-not-allowed inline-flex items-center justify-center"
        aria-label={busy && onCancel ? "Stop response" : "Send message"}
        title={busy && onCancel ? "Stop response" : "Send message"}
      >
        <Icon name={busy && onCancel ? "stop" : "play"} size={16} />
      </button>
    </form>
  );
}
