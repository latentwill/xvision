// Chat-rail REST + SSE wrappers around the dashboard's
// `/api/chat-rail/*` surface (Plan #11 Phase C).
//
// SSE: hand-rolls the parse over `fetch().body.getReader()` because
// EventSource is GET-only and we need to POST the chat body.

import { ApiError, apiFetch } from "./client";
import {
  createTrace,
  durationSince,
  safeId,
} from "@/lib/logger";
import type { UnifiedEvent, UnifiedPayloadKind } from "./unified-events";

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

type InlineChartKind =
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

type InlineChartSource = {
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

type ChatRunListItem = {
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

type ChoiceChipsContentBlock = {
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
  mode?: ChatSessionMode;
  history: ChatMessage[];
};

export type ChatSessionMode = "research" | "act";

export type SetSessionModeResp = {
  session_id: string;
  mode: ChatSessionMode;
};

export type ChatSessionSummary = {
  id: string;
  scope: ContextScope;
  mode?: ChatSessionMode;
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

export function setSessionMode(
  sessionId: string,
  mode: ChatSessionMode,
): Promise<SetSessionModeResp> {
  return apiFetch<SetSessionModeResp>(
    `/api/chat-rail/sessions/${encodeURIComponent(sessionId)}/mode`,
    {
      method: "POST",
      body: JSON.stringify({ mode }),
    },
  );
}

/**
 * @deprecated Legacy `WizardEvent` POST generator. The unified event stream
 * (`openUnifiedSessionStream`) is the source of truth for rail rows + the
 * trace dock (Phase 1.2/1.4). `streamChat` is retained ONLY to drive the
 * send/POST round-trip until the backend mirrors sends through the unified
 * log; rows must project from `stores/session-events.ts`, not from these
 * `WizardEvent`s. Do not add new row-rendering on this path.
 */
export async function* streamChat(
  req: {
    session_id: string;
    message: string;
    /// Explicit provider name (must exist in Settings → Providers).
    /// When omitted, the dashboard falls back to the default LLM provider.
    provider?: string;
    /// Explicit model id. When omitted, the dashboard falls back to
    /// [default_llm].model for the default provider.
    model?: string;
    /// Prompt/tool profile for the shared agent runtime.
    profile?: AgentProfile;
  },
  signal?: AbortSignal,
): AsyncGenerator<WizardEvent> {
  const trace = createTrace("chat", {
    stream_id: safeId(),
    session_id: req.session_id,
    provider: req.provider,
    model: req.model,
    profile: req.profile,
    message_len: req.message.length,
  });
  const started = performance.now();
  let eventIndex = 0;
  trace.info("chat.stream.start");

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
    trace.error("chat.stream.error", {
      status: res.status,
      code: body?.code,
      error_message: body?.message,
      duration_ms: durationSince(started),
    });
    throw new ApiError(
      res.status,
      body?.code ?? "http_error",
      body?.message ?? res.statusText ?? `HTTP ${res.status}`,
    );
  }
  trace.debug("chat.stream.response", {
    status: res.status,
    duration_ms: durationSince(started),
  });

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buf = "";
  try {
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
          const parsed = JSON.parse(json) as WizardEvent;
          eventIndex += 1;
          trace.debug("chat.stream.event", {
            event_index: eventIndex,
            type: parsed.type,
          });
          if (parsed.type === "tool_call") {
            trace.info("chat.tool.started", {
              event_index: eventIndex,
              tool: parsed.tool,
            });
          } else if (parsed.type === "tool_result") {
            trace.info("chat.tool.completed", {
              event_index: eventIndex,
              tool: parsed.tool,
            });
          } else if (parsed.type === "error") {
          trace.error("chat.stream.error", {
            event_index: eventIndex,
            error_message: parsed.message,
          });
          }
          yield parsed;
        } catch {
          trace.warn("chat.stream.malformed_frame", {
            event_index: eventIndex + 1,
            payload_bytes: json.length,
          });
        }
      }
    }
  } catch (err) {
    if (signal?.aborted) {
      trace.warn("chat.stream.abort", {
        events_count: eventIndex,
        duration_ms: durationSince(started),
      });
    } else {
      trace.error("chat.stream.error", {
        events_count: eventIndex,
        duration_ms: durationSince(started),
        error: err instanceof Error ? err.message : String(err),
      });
    }
    throw err;
  }
  if (signal?.aborted) {
    trace.warn("chat.stream.abort", {
      events_count: eventIndex,
      duration_ms: durationSince(started),
    });
  } else {
    trace.info("chat.stream.done", {
      events_count: eventIndex,
      duration_ms: durationSince(started),
    });
  }
}

// ---------------------------------------------------------------------------
// Unified session event stream (Phase 1.2/1.4).
//
// `GET /api/chat-rail/sessions/:id/stream?after_seq=<n>` (SSE, EventSource).
// Each frame is `event: <kind>\ndata: <UnifiedEvent JSON>\n\n` where the JSON
// matches `api/unified-events.ts` (adjacently tagged `{ kind, data }`). A
// control frame `event: replay_complete\ndata: {"last_seq":N}\n\n` separates
// the replayed history from the live tail.
//
// `after_seq = -1` (default) replays from the start. On reconnect we resume
// from the last seq we successfully rendered. Backoff mirrors the
// `SSE_BACKOFF_MS` schedule in `api/agent-runs.ts`.

const UNIFIED_SSE_BACKOFF_MS = [500, 1000, 2000, 4000, 8000];

/** Default `after_seq` — replay the whole log from the start. */
export const UNIFIED_STREAM_REPLAY_FROM_START = -1;

export type UnifiedStreamHandlers = {
  /** One parsed `UnifiedEvent` (replay or live tail). */
  onEvent: (ev: UnifiedEvent) => void;
  /** Fires when the replayed history ends and the live tail begins. */
  onReplayComplete?: (lastSeq: number) => void;
  /** Connection (re)opened — fires on first connect and each reconnect. */
  onOpen?: () => void;
  /** Terminal/transport error surfaced to the caller (reconnect continues). */
  onError?: (err: unknown) => void;
};

/** Close handle returned by `openUnifiedSessionStream`. */
export type UnifiedStreamHandle = () => void;

/**
 * Best-effort guard that a parsed object looks like a `UnifiedEvent`. We trust
 * the SSE `event:` name for the kind but validate the envelope shape so a
 * malformed frame is dropped rather than poisoning the reducer.
 */
function isUnifiedEvent(v: unknown): v is UnifiedEvent {
  if (typeof v !== "object" || v === null) return false;
  const o = v as Record<string, unknown>;
  return (
    typeof o.event_id === "string" &&
    typeof o.seq === "number" &&
    typeof o.payload === "object" &&
    o.payload !== null &&
    typeof (o.payload as { kind?: unknown }).kind === "string"
  );
}

/**
 * Open the unified SSE stream for a chat-rail session. Replays history from
 * `afterSeq` (pass `UNIFIED_STREAM_REPLAY_FROM_START` for the full log), then
 * streams the live tail. Reconnects with exponential backoff, resuming from
 * the highest seq seen so a dropped connection never replays already-rendered
 * frames. Returns a `close()` handle.
 */
export function openUnifiedSessionStream(
  sessionId: string,
  afterSeq: number,
  handlers: UnifiedStreamHandlers,
): UnifiedStreamHandle {
  const trace = createTrace("chat", {
    stream_id: safeId(),
    session_id: sessionId,
    transport: "unified_sse",
  });

  let closed = false;
  let attempt = 0;
  let source: EventSource | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  // Highest seq we've handed to `onEvent`. Resume point on reconnect.
  let lastSeqSeen = afterSeq;

  const urlFor = (after: number): string => {
    const id = encodeURIComponent(sessionId);
    return `/api/chat-rail/sessions/${id}/stream?after_seq=${after}`;
  };

  const handleUnified = (kind: UnifiedPayloadKind) => (ev: MessageEvent) => {
    let parsed: unknown;
    try {
      parsed = JSON.parse(ev.data as string);
    } catch {
      trace.warn("chat.unified.malformed_frame", { kind });
      return;
    }
    if (!isUnifiedEvent(parsed)) {
      trace.warn("chat.unified.invalid_envelope", { kind });
      return;
    }
    // Trust the wire `event:` name; the payload tag should agree.
    if (parsed.payload.kind !== kind) {
      trace.warn("chat.unified.kind_mismatch", {
        event_name: kind,
        payload_kind: parsed.payload.kind,
      });
    }
    if (parsed.seq > lastSeqSeen) lastSeqSeen = parsed.seq;
    handlers.onEvent(parsed);
  };

  const handleReplayComplete = (ev: MessageEvent) => {
    let lastSeq = lastSeqSeen;
    try {
      const data = JSON.parse(ev.data as string) as { last_seq?: number };
      if (typeof data.last_seq === "number") {
        lastSeq = data.last_seq;
        if (lastSeq > lastSeqSeen) lastSeqSeen = lastSeq;
      }
    } catch {
      // Control frame is malformed; fall back to the highest seq we saw.
    }
    trace.debug("chat.unified.replay_complete", { last_seq: lastSeq });
    handlers.onReplayComplete?.(lastSeq);
  };

  const connect = () => {
    if (closed) return;
    // Resume from the highest seq rendered so far (replays nothing already shown).
    source = new EventSource(urlFor(lastSeqSeen));

    source.addEventListener("open", () => {
      attempt = 0;
      trace.debug("chat.unified.open", { after_seq: lastSeqSeen });
      handlers.onOpen?.();
    });

    // One listener per UnifiedPayload kind. EventSource only delivers a frame
    // to the listener whose name matches the `event:` line, so we register
    // every kind the contract can emit.
    for (const kind of UNIFIED_EVENT_KINDS) {
      source.addEventListener(kind, handleUnified(kind) as EventListener);
    }
    source.addEventListener("replay_complete", handleReplayComplete as EventListener);

    source.addEventListener("error", (e) => {
      if (closed) return;
      handlers.onError?.(e);
      source?.close();
      source = null;
      const delay =
        UNIFIED_SSE_BACKOFF_MS[
          Math.min(attempt, UNIFIED_SSE_BACKOFF_MS.length - 1)
        ]!;
      attempt += 1;
      trace.warn("chat.unified.reconnect", { attempt, delay_ms: delay });
      reconnectTimer = setTimeout(connect, delay);
    });
  };

  connect();

  return () => {
    closed = true;
    if (reconnectTimer) clearTimeout(reconnectTimer);
    source?.close();
    source = null;
    trace.debug("chat.unified.close", { last_seq: lastSeqSeen });
  };
}

/**
 * Every `UnifiedPayload` kind the stream can emit, as SSE `event:` names.
 * Kept in sync with `api/unified-events.ts` (the contract). EventSource
 * dispatches by event name, so each must be registered as a listener.
 */
const UNIFIED_EVENT_KINDS: readonly UnifiedPayloadKind[] = [
  "session_created",
  "session_resumed",
  "session_interrupted",
  "session_completed",
  "session_failed",
  "run_started",
  "run_finished",
  "run_interrupted",
  "span_started",
  "span_finished",
  "model_call_finished",
  "assistant_message_started",
  "assistant_token_delta",
  "assistant_content_block",
  "assistant_message_done",
  "tool_requested",
  "tool_policy_checked",
  "tool_approved",
  "tool_started",
  "tool_delta",
  "tool_finished",
  "tool_failed",
  "tool_cancelled",
  "tool_denied",
  "broker_call_started",
  "broker_call_finished",
  "checkpoint_created",
  "checkpoint_restored",
  "checkpoint_restore_failed",
  "focus_loaded",
  "focus_edited",
  "focus_injected",
  "optimization_candidate_started",
  "optimization_candidate_metric",
  "optimization_candidate_selected",
  "optimization_completed",
  "memory_recall",
  "artifact_written",
  "supervisor_note",
  "engine_event",
  "error_missing_capability",
  "error_missing_tool",
  "error_invalid_schema",
  "error_provider_unavailable",
  "error_policy_denied",
  "error_persistence_failed",
  "sidecar_error",
  "backpressure_dropped",
];

// ---------------------------------------------------------------------------
// Scope derivation from the current location. The rail is mounted once in the
// app shell and intentionally keeps one shared workspace session across route
// changes. Page-specific context belongs in messages/tool calls, not separate
// rail sessions.
export function scopeFromPath(pathname: string, search = ""): ContextScope {
  void pathname;
  void search;
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

// ---------------------------------------------------------------------------
// Tool policy CRUD — /api/chat-rail/tool-policy
// Mirrors `xvision_engine::api::tool_policy` (EffectiveToolPolicy, KNOWN_TOOLS).

export type ToolClass = "read" | "write" | "dangerous";

export type EffectiveToolPolicy = {
  tool_name: string;
  class: ToolClass;
  enabled: boolean;
  auto_approve: boolean;
  is_override: boolean;
};

export const toolPolicyKeys = {
  all: () => ["tool-policy"] as const,
  list: (scope: string) => ["tool-policy", scope] as const,
};

export function listToolPolicies(scope = "global"): Promise<EffectiveToolPolicy[]> {
  return apiFetch<EffectiveToolPolicy[]>(
    `/api/chat-rail/tool-policy/effective?scope=${encodeURIComponent(scope)}`,
  );
}

export function setToolPolicy(
  toolName: string,
  policy: { enabled: boolean; auto_approve: boolean },
  scope = "global",
): Promise<void> {
  return apiFetch<void>("/api/chat-rail/tool-policy", {
    method: "PUT",
    body: JSON.stringify({
      tool_name: toolName,
      enabled: policy.enabled,
      auto_approve: policy.auto_approve,
      scope,
    }),
  });
}

export function deleteToolPolicy(toolName: string, scope = "global"): Promise<void> {
  return apiFetch<void>(
    `/api/chat-rail/tool-policy?tool_name=${encodeURIComponent(toolName)}&scope=${encodeURIComponent(scope)}`,
    { method: "DELETE" },
  );
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
