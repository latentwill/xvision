import type { ContentBlock } from "@/api/chat_rail";

export type Tool = {
  call: string;
  ok: boolean;
  summary: string;
  resultSummary?: string;
  /** True between tool_call and tool_result; drives the chip spinner. */
  pending?: boolean;
  /** Raw args from tool_call; consumed by tool narratives. */
  args?: unknown;
  /** Raw result from tool_result; consumed by tool narratives. */
  result?: unknown;
};

export type RichDisplayBlock = Exclude<
  ContentBlock,
  | { type: "text"; text: string }
  | { type: "tool_use"; id: string; name: string; input: unknown }
  | { type: "tool_result"; tool_use_id: string; content: string }
>;

export type RenderableBlock =
  | { kind: "text"; text: string }
  | { kind: "rich"; block: RichDisplayBlock }
  | { kind: "unsupported"; type: string };

export type AssistantBubble = {
  role: "assistant";
  blocks: RenderableBlock[];
  tools: Tool[];
};

type UserBubble = { role: "user"; text: string };

export type Bubble = UserBubble | AssistantBubble;
