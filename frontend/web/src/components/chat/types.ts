export type Tool = {
  call: string;
  ok: boolean;
  summary: string;
  /** True between tool_call and tool_result; drives the chip spinner. */
  pending?: boolean;
  /** Raw args from tool_call; consumed by tool narratives. */
  args?: unknown;
  /** Raw result from tool_result; consumed by tool narratives. */
  result?: unknown;
};

export type AssistantBubble = {
  role: "assistant";
  text: string;
  tools: Tool[];
};

export type UserBubble = { role: "user"; text: string };

export type Bubble = UserBubble | AssistantBubble;
