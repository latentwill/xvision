import { Suspense, useEffect, useMemo, useState, type ElementType } from "react";
import { Outlet, useLocation } from "react-router-dom";

import { headerLabel, placeholder, scopeFromPath } from "@/api/chat_rail";
import type { ChatRailProps } from "@/components/shell/ChatRail";
import { CommandPalette } from "@/components/shell/CommandPalette";
import { Icon } from "@/components/primitives/Icon";
import { MobileDrawer } from "@/components/mobile/MobileDrawer";
import { MobileFunctionsSheet } from "@/components/mobile/MobileFunctionsSheet";
import { MobileTopBar } from "@/components/mobile/MobileTopBar";
import { useUi } from "@/stores/ui";

export function MobileShell({
  ChatRailComponent,
}: {
  ChatRailComponent: ElementType<ChatRailProps>;
}) {
  const location = useLocation();
  const setDrawerOpen = useUi((s) => s.setMobileDrawerOpen);
  const setFunctionsOpen = useUi((s) => s.setMobileFunctionsOpen);
  const isHome = location.pathname === "/";
  const title = routeTitle(location.pathname);
  const [chatOpen, setChatOpen] = useState(false);
  const scope = useMemo(
    () => scopeFromPath(location.pathname, location.search),
    [location.pathname, location.search],
  );

  useEffect(() => {
    setChatOpen(false);
  }, [location.pathname, location.search]);

  return (
    <div className="h-[100dvh] bg-bg text-text overflow-hidden flex flex-col">
      <MobileTopBar
        title={isHome ? undefined : title}
        onMenu={() => setDrawerOpen(true)}
      />
      <main className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden px-4 pt-4 pb-24">
        <Outlet />
      </main>
      <>
        <ChatPill
          context={headerLabel(scope)}
          placeholder={placeholder(scope)}
          onOpen={() => setChatOpen(true)}
        />
        {chatOpen && (
          <MobileChatOverlay
            context={headerLabel(scope)}
            onClose={() => setChatOpen(false)}
            onOpenActions={() => setFunctionsOpen(true)}
            ChatRailComponent={ChatRailComponent}
          />
        )}
      </>
      <MobileDrawer />
      <MobileFunctionsSheet />
      <CommandPalette />
    </div>
  );
}

function ChatPill({
  context,
  placeholder,
  onOpen,
}: {
  context: string;
  placeholder: string;
  onOpen: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onOpen}
      className="fixed left-3 right-3 bottom-[max(12px,env(safe-area-inset-bottom))] z-30 h-[58px] md:hidden rounded-full border border-border-strong bg-surface-card shadow-2xl shadow-black/35 flex items-center gap-3 px-3 text-left"
      aria-label="Open chat"
    >
      <span className="w-9 h-9 rounded-full bg-gold/10 border border-gold/30 flex items-center justify-center text-gold flex-shrink-0">
        <Icon name="pulse" size={17} />
      </span>
      <span className="flex-1 min-w-0">
        <span className="block text-[11px] font-mono text-text-3 truncate">
          {context}
        </span>
        <span className="block text-[13px] text-text-2 truncate">
          {placeholder}
        </span>
      </span>
      <span className="w-8 h-8 rounded-full bg-surface-panel border border-border flex items-center justify-center text-text-2 flex-shrink-0">
        <Icon name="arrow" size={15} />
      </span>
    </button>
  );
}

function MobileChatOverlay({
  context,
  onClose,
  onOpenActions,
  ChatRailComponent,
}: {
  context: string;
  onClose: () => void;
  onOpenActions: () => void;
  ChatRailComponent: ElementType<ChatRailProps>;
}) {
  return (
    <section className="fixed inset-0 z-40 md:hidden bg-bg flex flex-col">
      <header className="h-[calc(52px+env(safe-area-inset-top))] pt-[env(safe-area-inset-top)] flex items-center gap-3 px-3 border-b border-border-soft bg-bg flex-shrink-0">
        <button
          type="button"
          onClick={onClose}
          className="w-9 h-9 rounded-full flex items-center justify-center text-text-2 hover:text-text hover:bg-surface-hover"
          aria-label="Close chat"
        >
          <Icon name="plus" size={18} className="rotate-45" />
        </button>
        <div className="flex-1 min-w-0">
          <div className="text-[11px] font-mono text-text-3 truncate">
            {context}
          </div>
          <div className="font-sans text-[20px] leading-tight text-text">
            Chat
          </div>
        </div>
      </header>
      <div className="flex-1 min-h-0">
        <Suspense fallback={null}>
          <ChatRailComponent
            variant="panel"
            showHeader={false}
            onOpenActions={onOpenActions}
          />
        </Suspense>
      </div>
    </section>
  );
}

function routeTitle(pathname: string): string {
  if (pathname.startsWith("/eval-runs/")) return "Eval run";
  if (pathname === "/eval-runs") return "Eval runs";
  if (pathname.startsWith("/strategies")) return "Strategies";
  if (pathname.startsWith("/agents")) return "Agents";
  if (pathname.startsWith("/authoring")) return "Authoring";
  if (pathname.startsWith("/settings")) return "Settings";
  return "xvn";
}
