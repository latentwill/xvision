import { useNavigate } from "react-router-dom";

import { InlineChartCard } from "@/components/chat/inline-chart/InlineChartCard";
import { ChatActionCard } from "@/components/chat/cards/ChatActionCard";
import { ChatRunListCard } from "@/components/chat/cards/ChatRunListCard";
import { ChatStrategyCard } from "@/components/chat/cards/ChatStrategyCard";
import { runInlineAction } from "@/components/chat/cards/actions";
import type { RichDisplayBlock } from "./types";

function isSafeHref(href: string): boolean {
  const colon = href.indexOf(":");
  const slash = href.indexOf("/");
  if (colon === -1 || (slash !== -1 && slash < colon)) {
    return true;
  }
  const scheme = href.slice(0, colon + 1).toLowerCase();
  return scheme === "http:" || scheme === "https:" || scheme === "mailto:";
}

export function ContentBlockView({ block }: { block: RichDisplayBlock }) {
  switch (block.type) {
    case "inline_chart":
      return <InlineChartCard payload={block} />;
    case "run_list":
      return <ChatRunListCard payload={block} />;
    case "strategy_card":
      return <ChatStrategyCard payload={block} />;
    case "action_card":
      return <ChatActionCard payload={block} />;
    case "choice_chips":
      return <ChoiceChips chips={block.chips} />;
    default:
      return <UnsupportedBlock type={(block as { type?: string }).type} />;
  }
}

function ChoiceChips({
  chips,
}: {
  chips: Array<{ label: string; href?: string | null; command?: string | null }>;
}) {
  const navigate = useNavigate();
  return (
    <div className="flex flex-wrap gap-1">
      {chips.map((chip) => {
        const key = `${chip.label}:${chip.href ?? chip.command ?? ""}`;
        const className =
          "rounded-full border border-border-soft px-2.5 py-1 text-[11px] text-text-2";
        if (chip.href && isSafeHref(chip.href)) {
          return (
            <a key={key} href={chip.href} className={className}>
              {chip.label}
            </a>
          );
        }
        if (chip.command) {
          return (
            <button
              key={key}
              type="button"
              onClick={() => runInlineAction(chip, navigate)}
              className={`${className} hover:text-text`}
            >
              {chip.label}
            </button>
          );
        }
        return (
          <span key={key} className={className}>
            {chip.label}
          </span>
        );
      })}
    </div>
  );
}

function UnsupportedBlock({ type }: { type?: string }) {
  return (
    <div className="rounded border border-border-soft bg-surface-elev px-2 py-1 text-[12px] text-text-3">
      Unsupported chat block: <span className="font-mono">{type ?? "unknown"}</span>
    </div>
  );
}
