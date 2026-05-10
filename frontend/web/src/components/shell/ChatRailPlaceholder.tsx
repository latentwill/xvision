// Phase 1 stub. The real chat rail (collapsible, persistent across routes)
// lands with the chat-rail-persistence plan; for now we render the collapsed
// edge so the layout reserves space and the seam matches the prototype.

import { Icon } from "@/components/primitives/Icon";

export function ChatRailPlaceholder() {
  return (
    <aside
      className="hidden xl:flex w-[44px] flex-col items-center gap-3 border-l border-border-soft bg-surface-sidebar py-4"
      aria-label="Chat rail (placeholder)"
    >
      <button
        className="w-8 h-8 rounded-full flex items-center justify-center text-text-3 hover:text-text border border-border-soft"
        title="Open agent chat (coming soon)"
        disabled
      >
        <Icon name="pulse" size={14} />
      </button>
    </aside>
  );
}
