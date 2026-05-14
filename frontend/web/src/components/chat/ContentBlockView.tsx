import { InlineChartCard } from "@/components/chat/inline-chart/InlineChartCard";
import { ChatActionCard } from "@/components/chat/cards/ChatActionCard";
import { ChatRunListCard } from "@/components/chat/cards/ChatRunListCard";
import { ChatStrategyCard } from "@/components/chat/cards/ChatStrategyCard";
import type { ContentBlock } from "@/api/chat_rail";

export type RichDisplayBlock = Exclude<
  ContentBlock,
  | { type: "text"; text: string }
  | { type: "tool_use"; id: string; name: string; input: unknown }
  | { type: "tool_result"; tool_use_id: string; content: string }
>;

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
  return (
    <div className="flex flex-wrap gap-1">
      {chips.map((chip) => (
        <span
          key={`${chip.label}:${chip.href ?? chip.command ?? ""}`}
          className="rounded-full border border-border-soft px-2.5 py-1 text-[11px] text-text-2"
        >
          {chip.label}
        </span>
      ))}
    </div>
  );
}

function UnsupportedBlock({ type }: { type?: string }) {
  if (!import.meta.env.DEV) return null;
  return (
    <div className="rounded border border-border-soft bg-surface-elev px-2 py-1 text-[12px] text-text-3">
      Unsupported chat block: <span className="font-mono">{type ?? "unknown"}</span>
    </div>
  );
}
