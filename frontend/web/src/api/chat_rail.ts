// Chat-rail REST + SSE wrappers around the dashboard's
// `/api/chat-rail/*` surface (Plan #11 Phase C).
//
// SSE: hand-rolls the parse over `fetch().body.getReader()` because
// EventSource is GET-only and we need to POST the chat body.

import { ApiError, apiFetch } from "./client";

// ContextScope mirrors `xvision_engine::chat_session::ContextScope`'s
// serde tagged-union shape: `{ scope: <variant>, ...fields }`.
export type ContextScope =
  | { scope: "workspace" }
  | { scope: "route"; route: string }
  | { scope: "run"; run_id: string }
  | { scope: "strategy"; draft_id: string }
  | { scope: "deployment"; deployment_id: string }
  | { scope: "compare"; run_ids: string[] }
  | { scope: "journal_filter"; kinds: string[] }
  | { scope: "selection"; items: string[] }
  | { scope: "seed"; seed_id: string };

export type ChatMessage = {
  id: string;
  session_id: string;
  seq: number;
  role: "user" | "assistant" | string;
  content_blocks: ContentBlock[];
  ts: string;
};

// Mirrors xvision_engine::agent::llm::ContentBlock — same tagged-union
// (#[serde(tag = "type")]) over text / tool_use / tool_result, extended
// with chat-session rich display blocks built by trusted server-side tools.
export type ContentBlock =
  | { type: "text"; text: string }
  | { type: "tool_use"; id: string; name: string; input: unknown }
  | { type: "tool_result"; tool_use_id: string; content: string }
  | InlineChartContentBlock
  | RunListContentBlock
  | StrategyCardContentBlock
  | ActionCardContentBlock
  | ChoiceChipsContentBlock;

export type InlineTone =
  | "default"
  | "gold"
  | "info"
  | "warn"
  | "danger"
  | "muted";

export type InlineChartKind =
  | "equity"
  | "compare"
  | "histogram"
  | "drawdown"
  | "sparkline"
  | "trade_markers";

export type InlinePoint = {
  x: number;
  y: number;
  label?: string | null;
};

export type InlineChartSeries = {
  id: string;
  label: string;
  tone?: InlineTone | null;
  points: InlinePoint[];
};

export type InlineMetric = {
  label: string;
  value: string;
  unit?: string | null;
  tone?: InlineTone | null;
};

export type InlineAction = {
  label: string;
  href?: string | null;
  command?: string | null;
};

export type InlineChartSource = {
  label: string;
  href?: string | null;
  run_id?: string | null;
  strategy_id?: string | null;
};

export type InlineChartContentBlock = {
  type: "inline_chart";
  chart_id: string;
  kind: InlineChartKind;
  title: string;
  subtitle?: string | null;
  primary_metric?: InlineMetric | null;
  metrics: InlineMetric[];
  series: InlineChartSeries[];
  source?: InlineChartSource | null;
  actions: InlineAction[];
  a11y_summary: string;
  downsampled: boolean;
};

export type RunListContentBlock = {
  type: "run_list";
  title: string;
  runs: ChatRunListItem[];
  actions: InlineAction[];
};

export type ChatRunListItem = {
  rank: number;
  run_id: string;
  strategy_id?: string | null;
  scenario?: string | null;
  return_pct?: number | null;
  sharpe?: number | null;
  sparkline?: InlinePoint[] | null;
};

export type StrategyCardContentBlock = {
  type: "strategy_card";
  strategy_id: string;
  title: string;
  subtitle?: string | null;
  status?: string | null;
  metrics: InlineMetric[];
  tags: string[];
  actions: InlineAction[];
};

export type ActionCardContentBlock = {
  type: "action_card";
  action_id: string;
  title: string;
  body: string;
  confirm: InlineAction;
  cancel?: InlineAction | null;
};

export type ChoiceChipsContentBlock = {
  type: "choice_chips";
  chips: InlineAction[];
};

export type WizardEvent =
  | { type: "token"; text: string }
  | { type: "tool_call"; tool: string; args: unknown }
  | { type: "tool_result"; tool: string; result: unknown }
  | { type: "content_block"; block: ContentBlock }
  | { type: "done"; draft_id?: string | null }
  | { type: "error"; message: string };

export type AgentProfile = "workspace" | "strategy_setup";

export type ResolveSessionResp = {
  session_id: string;
  history: ChatMessage[];
};

export type ChatSessionSummary = {
  id: string;
  scope: ContextScope;
  started_at: string;
  last_activity_at: string;
};

export function createSession(
  scope: ContextScope,
): Promise<ResolveSessionResp> {
  return apiFetch<ResolveSessionResp>("/api/chat-rail/sessions", {
    method: "POST",
    body: JSON.stringify({ scope }),
  });
}

/// Resolve the chat-rail session for the current scope. Server returns
/// the most-recent session matching the scope (with its full history),
/// or creates a fresh empty session if no match exists. Always lands a
/// usable id — no client-side cache to go stale.
export function resolveSession(
  scope: ContextScope,
): Promise<ResolveSessionResp> {
  return apiFetch<ResolveSessionResp>(
    "/api/chat-rail/sessions/resolve",
    {
      method: "POST",
      body: JSON.stringify({ scope }),
    },
  );
}

export async function deleteSession(sessionId: string): Promise<void> {
  await apiFetch<void>(
    `/api/chat-rail/sessions/${encodeURIComponent(sessionId)}`,
    { method: "DELETE" },
  );
}

export function listSessions(): Promise<ChatSessionSummary[]> {
  return apiFetch<ChatSessionSummary[]>("/api/chat-rail/sessions");
}

export function loadSessionHistory(sessionId: string): Promise<ChatMessage[]> {
  return apiFetch<ChatMessage[]>(
    `/api/chat-rail/sessions/${encodeURIComponent(sessionId)}/history`,
  );
}

export async function* streamChat(
  req: {
    session_id: string;
    message: string;
    /// Explicit provider name (must exist in Settings → Providers).
    /// When omitted, the dashboard falls back to the intern's default
    /// provider.
    provider?: string;
    /// Explicit model id. When omitted, the dashboard falls back to
    /// [intern].model for the default provider.
    model?: string;
    /// Prompt/tool profile for the shared agent runtime.
    profile?: AgentProfile;
  },
  signal?: AbortSignal,
): AsyncGenerator<WizardEvent> {
  console.info("[chat-rail] streamChat", {
    session_id: req.session_id,
    provider: req.provider,
    model: req.model,
    profile: req.profile,
    message_len: req.message.length,
  });
  const res = await fetch("/api/chat-rail/chat", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      accept: "text/event-stream",
    },
    body: JSON.stringify(req),
    signal,
  });
  if (!res.ok || !res.body) {
    let body: { code?: string; message?: string } | undefined;
    try {
      body = await res.json();
    } catch {
      // not JSON
    }
    console.error("[chat-rail] streamChat failed", {
      status: res.status,
      code: body?.code,
      message: body?.message,
    });
    throw new ApiError(
      res.status,
      body?.code ?? "http_error",
      body?.message ?? res.statusText ?? `HTTP ${res.status}`,
    );
  }

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buf = "";
  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    const frames = buf.split("\n\n");
    buf = frames.pop() ?? "";
    for (const frame of frames) {
      const dataLine = frame.split("\n").find((l) => l.startsWith("data:"));
      if (!dataLine) continue;
      const json = dataLine.slice(5).trim();
      if (!json) continue;
      try {
        yield JSON.parse(json) as WizardEvent;
      } catch {
        // skip malformed
      }
    }
  }
}

// ---------------------------------------------------------------------------
// Scope derivation from the current location. The rail is mounted once in the
// app shell and intentionally keeps one shared workspace session across route
// changes. Page-specific context belongs in messages/tool calls, not separate
// rail sessions.
export function scopeFromPath(pathname: string): ContextScope {
  void pathname;
  return { scope: "workspace" };
}

/// Stable key for session state. Workspace is the only rail-owned scope today;
/// the other variants remain for server/API compatibility with historical
/// sessions and future explicit context handoffs.
export function scopeKey(scope: ContextScope): string {
  switch (scope.scope) {
    case "run":
      return `run:${scope.run_id}`;
    case "strategy":
      return `strategy:${scope.draft_id}`;
    case "deployment":
      return `deployment:${scope.deployment_id}`;
    case "route":
      return `route:${scope.route}`;
    case "compare":
      return `compare:${scope.run_ids.join(",")}`;
    case "journal_filter":
      return `journal:${scope.kinds.join(",")}`;
    case "selection":
      return `sel:${scope.items.join(",")}`;
    case "seed":
      return `seed:${scope.seed_id}`;
    case "workspace":
      return "workspace";
  }
}

// Header label + placeholder + quick replies are needed by the rail UI
// without round-tripping to the engine. These mirror
// `xvision_engine::chat_session::ContextScope`'s impl methods exactly.

export function headerLabel(scope: ContextScope): string {
  switch (scope.scope) {
    case "workspace":
      return "Whole workspace";
    case "route":
      return `This page · ${scope.route}`;
    case "run":
      return `Run · ${scope.run_id}`;
    case "strategy":
      return `Editing · ${scope.draft_id}`;
    case "deployment":
      return `Deployment · ${scope.deployment_id}`;
    case "compare":
      return `Comparing ${scope.run_ids.length} runs`;
    case "journal_filter":
      return scope.kinds.length === 0
        ? "Journal"
        : `Journal · ${scope.kinds.join(", ")}`;
    case "selection":
      return `Selection · ${scope.items.length} items`;
    case "seed":
      return `Seed · ${scope.seed_id}`;
  }
}

export function quickReplies(scope: ContextScope): string[] {
  switch (scope.scope) {
    case "workspace":
      return [
        "What needs my attention?",
        "Pick a draft to work on",
        "Summarize this week",
      ];
    case "run":
      return [
        "Why did it underperform?",
        "Compare to its baseline",
        "Suggest a variant to draft",
      ];
    case "strategy":
      return [
        "Improve this prompt",
        "Why is this slot expensive?",
        "Suggest a tool to add",
        "Diff vs template",
      ];
    case "deployment":
      return [
        "Is this drift real?",
        "Should I pause it?",
        "Draft a variant from yesterday's vetoes",
      ];
    case "compare":
      return [
        "What do the winners share?",
        "Why did the worst run underperform?",
        "Suggest a synthesis variant",
      ];
    case "journal_filter":
      return [
        "Summarize what I've learned this week",
        "What's my most repeated mistake?",
        "Suggest a variant based on recent findings",
      ];
    case "selection":
      return [
        "Compare these",
        "What do they have in common?",
        "Draft a variant that synthesizes them",
      ];
    case "seed":
      return ["Use this seed as the starting point", "Show what was different"];
    case "route":
      switch (scope.route) {
        case "/strategies":
          return [
            "Help me pick which to work on",
            "Which has the worst recent eval?",
            "Suggest a fork from the top-of-list",
          ];
        case "/eval/runs":
          return [
            "Pick the most suspicious run",
            "Find runs that disagree on the same scenario",
            "Suggest a new scenario to test",
          ];
        default:
          return [];
      }
  }
}

export function placeholder(scope: ContextScope): string {
  switch (scope.scope) {
    case "workspace":
      return "Ask anything about your workspace…";
    case "route":
      return "Ask about this page…";
    case "run":
      return "Ask about this run…";
    case "strategy":
      return "Edit this slot…";
    case "deployment":
      return "Ask about this deployment…";
    case "compare":
      return "Ask about this comparison…";
    case "journal_filter":
      return "Ask about your journal…";
    case "selection":
      return "Ask about your selection…";
    case "seed":
      return "Refine this seed…";
  }
}
