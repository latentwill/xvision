import { useNavigate } from "react-router-dom";

import type { ActionCardContentBlock } from "@/api/chat_rail";
import { runInlineAction } from "./actions";

export function ChatActionCard({ payload }: { payload: ActionCardContentBlock }) {
  const navigate = useNavigate();

  return (
    <article
      role="group"
      aria-label={payload.title}
      className="rounded-md border border-gold/30 bg-gold/[0.06] overflow-hidden"
    >
      <header className="px-3 pt-3 pb-1">
        <h3 className="m-0 text-[13px] font-semibold text-text">
          {payload.title}
        </h3>
      </header>
      <p className="m-0 px-3 pb-3 text-[12px] leading-snug text-text-2">
        {payload.body}
      </p>
      <footer className="px-3 py-2 border-t border-gold/20 flex justify-end gap-1.5">
        {payload.cancel ? (
          <button
            type="button"
            onClick={() => runInlineAction(payload.cancel!, navigate)}
            className="px-2.5 py-1 rounded border border-border-soft text-[11px] text-text-2 hover:text-text"
          >
            {payload.cancel.label}
          </button>
        ) : null}
        <button
          type="button"
          onClick={() => runInlineAction(payload.confirm, navigate)}
          className="px-2.5 py-1 rounded bg-gold text-bg text-[11px] font-medium hover:bg-gold-soft active:scale-[0.96]"
        >
          {payload.confirm.label}
        </button>
      </footer>
    </article>
  );
}
