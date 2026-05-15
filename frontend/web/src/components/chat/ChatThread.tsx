import { useEffect, useRef } from "react";

import { ChatBubble } from "./ChatBubble";
import type { Bubble } from "./types";

export function ChatThread({
  bubbles,
  isStreaming,
}: {
  bubbles: Bubble[];
  isStreaming: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (typeof el.scrollTo === "function") {
      el.scrollTo({
        top: el.scrollHeight,
        behavior: "smooth",
      });
      return;
    }
    el.scrollTop = el.scrollHeight;
  }, [bubbles]);

  return (
    <div
      ref={ref}
      className="xvn-scroll xvn-scroll--always rail flex-1 min-h-0 px-4 py-3 flex flex-col gap-2"
    >
      {bubbles.length === 0 ? (
        <div className="text-text-3 italic text-[13px] text-center py-4">
          No messages yet. Ask the agent something - it has tools for the
          authoring loop.
        </div>
      ) : (
        bubbles.map((b, i) => (
          <ChatBubble
            key={i}
            bubble={b}
            isLast={i === bubbles.length - 1}
            isStreaming={isStreaming}
          />
        ))
      )}
    </div>
  );
}
