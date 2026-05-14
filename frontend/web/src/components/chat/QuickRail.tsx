import type { ContextScope } from "@/api/chat_rail";
import { quickReplies } from "@/api/chat_rail";

export function QuickRail({
  scope,
  disabled,
  onPick,
}: {
  scope: ContextScope;
  disabled: boolean;
  onPick: (s: string) => void;
}) {
  const replies = quickReplies(scope);
  if (replies.length === 0) return null;
  return (
    <div className="border-t border-border-soft px-3 py-2 flex flex-wrap gap-1">
      {replies.map((r) => (
        <button
          key={r}
          disabled={disabled}
          onClick={() => onPick(r)}
          className="text-[11px] text-text-2 hover:text-text border border-border-soft rounded-full px-2.5 py-1 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {r}
        </button>
      ))}
    </div>
  );
}
