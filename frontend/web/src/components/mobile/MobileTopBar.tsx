import { Icon } from "@/components/primitives/Icon";

export function MobileTopBar({
  title,
  context,
  onMenu,
}: {
  title?: string;
  context?: string;
  onMenu: () => void;
}) {
  return (
    <header className="h-[calc(52px+env(safe-area-inset-top))] pt-[env(safe-area-inset-top)] pl-[max(0.75rem,env(safe-area-inset-left))] pr-[max(0.75rem,env(safe-area-inset-right))] flex items-center gap-2 border-b border-border-soft bg-bg flex-shrink-0">
      <button
        type="button"
        onClick={onMenu}
        className="w-9 h-9 rounded-full flex items-center justify-center text-text-2 hover:text-text hover:bg-surface-hover"
        aria-label="Open navigation"
      >
        <Icon name="list" size={18} />
      </button>
      <div className="flex-1 min-w-0 flex items-center justify-center">
        {title ? (
          <div className="font-serif text-[22px] font-medium text-text truncate">
            {title}
          </div>
        ) : context ? (
          <div className="max-w-[180px] flex items-center gap-1.5 px-2.5 py-1.5 rounded-full border border-border bg-surface-card text-[12px] text-text truncate">
            <span className="w-1.5 h-1.5 rounded-full bg-gold flex-shrink-0" />
            <span className="font-mono truncate">{context}</span>
          </div>
        ) : (
          <span className="font-serif italic font-medium text-[24px] tracking-tight text-text">
            xvn
          </span>
        )}
      </div>
      <button
        type="button"
        className="relative w-9 h-9 rounded-full flex items-center justify-center text-text-2 hover:text-text hover:bg-surface-hover"
        aria-label="Agent status"
      >
        <Icon name="pulse" size={18} />
        <span className="absolute top-2 right-2 w-1.5 h-1.5 rounded-full bg-gold border border-bg" />
      </button>
    </header>
  );
}
