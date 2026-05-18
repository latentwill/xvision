// The persistent chat rail — collapsed 44px icon strip on the right edge,
// expanded 360px panel showing the agent thread for the current scope.
// Plan #11 Phase D Tasks 5-6, adapted to React (the original spec
// targeted handlebars + chat_rail.js).
//
// Scope is derived from the current location. One session per
// scope-key, cached in localStorage so navigating away and back resumes
// the conversation.

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { useLocation } from "react-router-dom";

import { useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";

import { strategyKeys } from "@/api/strategies";
import { scenarioKeys } from "@/api/scenarios";
import { agentKeys } from "@/api/agents";
import { evalKeys } from "@/api/eval";

import { ChatComposer } from "@/components/chat/ChatComposer";
import { ChatThread } from "@/components/chat/ChatThread";
import { QuickRail } from "@/components/chat/QuickRail";
import type {
  AssistantBubble,
  Bubble,
  RenderableBlock,
  RichDisplayBlock,
  Tool,
} from "@/components/chat/types";
import { Icon } from "@/components/primitives/Icon";
import { ModelPicker } from "@/components/ModelPicker";
import { ApiError } from "@/api/client";
import {
  safeStorageGet,
  safeStorageRemove,
  safeStorageSet,
} from "@/lib/storage";
import {
  type ChatMessage,
  type ContentBlock,
  type ContextScope,
  type WizardEvent,
  createSession,
  headerLabel,
  listSessions,
  loadSessionHistory,
  placeholder,
  resolveSession,
  scopeFromPath,
  scopeKey,
  streamChat,
} from "@/api/chat_rail";
import { listProviders, settingsKeys } from "@/api/settings";
import type { ProviderRow } from "@/api/types.gen";

const RAIL_OPEN_LS = "xvn.chat_rail.open";
const RAIL_PROVIDER_LS = "xvn.chat_rail.provider";
const RAIL_MODEL_LS = "xvn.chat_rail.model";

export type ChatRailProps = {
  variant?: "desktop" | "panel";
  className?: string;
  showHeader?: boolean;
  onOpenActions?: () => void;
};

export function ChatRail({
  variant = "desktop",
  className = "",
  showHeader = true,
  onOpenActions,
}: ChatRailProps) {
  const location = useLocation();
  const qc = useQueryClient();
  const scope = useMemo<ContextScope>(
    () => scopeFromPath(location.pathname, location.search),
    [location.pathname, location.search],
  );
  const key = useMemo(() => scopeKey(scope), [scope]);

  const [open, setOpen] = useState<boolean>(() => {
    return safeStorageGet(RAIL_OPEN_LS) === "1";
  });
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [bubbles, setBubbles] = useState<Bubble[]>([]);
  const [input, setInput] = useState("");
  const [isStreaming, setIsStreaming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [providerName, setProviderName] = useState<string | null>(
    () => safeStorageGet(RAIL_PROVIDER_LS),
  );
  const [modelId, setModelId] = useState<string>(
    () => safeStorageGet(RAIL_MODEL_LS) ?? "",
  );
  const abortRef = useRef<AbortController | null>(null);
  const lastScopeKeyRef = useRef<string | null>(null);

  const providers = useQuery({
    queryKey: settingsKeys.providers(),
    queryFn: listProviders,
    enabled: variant === "panel" || open,
  });
  const sessionsQ = useQuery({
    queryKey: ["chat-rail", "sessions"],
    queryFn: listSessions,
    enabled: variant === "panel" || open,
    refetchInterval: 5000,
  });
  // Auto-pick the first enabled (provider, model) once the catalog loads
  // so users who configured a provider can chat without diving into the
  // picker. If the operator hasn't enabled any models yet, the picker
  // shows a "visit Settings" hint.
  useEffect(() => {
    if (providerName && modelId) return;
    const rows = providers.data?.providers ?? [];
    const candidates = rows.filter(
      (p) => p.api_key_set && !p.synthetic && p.enabled_models.length > 0,
    );
    const pick = candidates[0];
    if (!pick) return;
    const m = pick.enabled_models[0];
    setProviderName(pick.name);
    setModelId(m);
    safeStorageSet(RAIL_PROVIDER_LS, pick.name);
    safeStorageSet(RAIL_MODEL_LS, m);
  }, [providerName, modelId, providers.data]);

  // Persist open/close so the rail stays in the user's chosen state across
  // route changes (and reloads).
  useEffect(() => {
    if (variant !== "desktop") return;
    safeStorageSet(RAIL_OPEN_LS, open ? "1" : "0");
  }, [open, variant]);

  // When the rail is open and the scope changes, resolve a session for
  // the current scope. The server owns session lifecycle — the rail
  // never holds a stale id across DB resets or fresh deploys.
  useEffect(() => {
    if (variant === "desktop" && !open) return;
    if (lastScopeKeyRef.current === key && sessionId) return;
    lastScopeKeyRef.current = key;

    let cancelled = false;
    (async () => {
      setError(null);
      try {
        const resolved = await resolveSession(scope);
        if (cancelled) return;
        setSessionId(resolved.session_id);
        setBubbles(historyToBubbles(resolved.history));
      } catch (e) {
        if (cancelled) return;
        setError(formatErr(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [open, key, scope, sessionId, variant]);

  // Cancel any in-flight stream when rail closes or component unmounts.
  useEffect(
    () => () => {
      abortRef.current?.abort();
    },
    [],
  );

  const send = useCallback(
    async (text: string) => {
      if (!sessionId || !text.trim() || isStreaming) return;
      setError(null);
      const userText = text.trim();
      setInput("");
      setBubbles((b) => [
        ...b,
        { role: "user", text: userText },
        { role: "assistant", blocks: [{ kind: "text", text: "" }], tools: [] },
      ]);
      setIsStreaming(true);
      const ctrl = new AbortController();
      abortRef.current = ctrl;
      try {
        for await (const ev of streamChat(
          {
            session_id: sessionId,
            message: userText,
            provider: providerName ?? undefined,
            model: modelId.trim() || undefined,
            profile: "workspace",
          },
          ctrl.signal,
        )) {
          applyEvent(setBubbles, ev);
          invalidateForToolResult(qc, ev);
        }
      } catch (e) {
        if ((e as Error).name === "AbortError") return;
        setError(formatErr(e));
      } finally {
        setIsStreaming(false);
        abortRef.current = null;
      }
    },
    [sessionId, isStreaming, providerName, modelId, qc],
  );

  const stopStreaming = useCallback(() => {
    abortRef.current?.abort();
  }, []);

  const startFresh = useCallback(async () => {
    abortRef.current?.abort();
    setInput("");
    setBubbles([]);
    setError(null);
    try {
      const created = await createSession(scope);
      setSessionId(created.session_id);
      setBubbles(historyToBubbles(created.history));
      lastScopeKeyRef.current = key;
      void sessionsQ.refetch();
    } catch (e) {
      setError(formatErr(e));
    }
  }, [key, scope, sessionsQ]);

  const recentScopeSessions = useMemo(() => {
    return (sessionsQ.data ?? [])
      .filter((s) => scopeKey(s.scope) === key)
      .slice(0, 8);
  }, [key, sessionsQ.data]);

  if (variant === "desktop" && !open) {
    return (
      <aside
        className="hidden xl:flex w-[44px] flex-col items-center gap-3 h-screen sticky top-0 border-l border-border-soft bg-surface-sidebar py-4"
        aria-label="Chat rail"
      >
        <button
          className="w-8 h-8 rounded-full flex items-center justify-center text-text-3 hover:text-text border border-border-soft"
          title="Open agent chat (⌘\\)"
          onClick={() => setOpen(true)}
        >
          <Icon name="pulse" size={14} />
        </button>
      </aside>
    );
  }

  return (
    <aside
      className={[
        variant === "desktop"
          ? "hidden xl:flex w-[380px] flex-col h-screen sticky top-0 border-l border-border-soft bg-surface-sidebar"
          : "flex w-full flex-col h-full min-h-0 bg-surface-sidebar",
        className,
      ].join(" ")}
      aria-label="Chat rail"
    >
      {showHeader && (
        <header className="px-4 py-3 border-b border-border-soft flex items-center justify-between gap-2">
          <div className="text-[12px] text-text-2 truncate">
            Context · <span className="text-text">{headerLabel(scope)}</span>
          </div>
          <div className="flex items-center gap-1">
            <button
              className="text-[11px] text-text-3 hover:text-text border border-border-soft rounded-sm px-2 py-1"
              onClick={startFresh}
              title="Start a new conversation in this context"
            >
              New chat
            </button>
            {variant === "desktop" && (
              <button
                className="text-text-3 hover:text-text"
                onClick={() => setOpen(false)}
                title="Collapse rail"
              >
                <Icon name="chevR" size={14} />
              </button>
            )}
          </div>
        </header>
      )}
      {showHeader && recentScopeSessions.length > 0 && (
        <div className="px-4 py-2 border-b border-border-soft bg-surface-2/20">
          <div className="text-[11px] text-text-3 mb-1">Conversation history</div>
          <div className="space-y-1">
            {recentScopeSessions.map((s) => (
              <button
                key={s.id}
                onClick={async () => {
                  try {
                    setSessionId(s.id);
                    const h = await loadSessionHistory(s.id);
                    setBubbles(historyToBubbles(h));
                  } catch (e) {
                    setError(formatErr(e));
                  }
                }}
                className={`w-full text-left rounded px-2 py-1 text-[11px] border ${
                  s.id === sessionId
                    ? "border-gold/40 text-text bg-gold/5"
                    : "border-border-soft text-text-2 hover:text-text"
                }`}
              >
                {new Date(s.last_activity_at).toLocaleString()}
              </button>
            ))}
          </div>
        </div>
      )}

      <RailModelBar
        rows={providers.data?.providers ?? []}
        loading={providers.isPending}
        provider={providerName}
        model={modelId}
        onChange={(p, m) => {
          setProviderName(p);
          setModelId(m);
          if (p) safeStorageSet(RAIL_PROVIDER_LS, p);
          else safeStorageRemove(RAIL_PROVIDER_LS);
          if (m) safeStorageSet(RAIL_MODEL_LS, m);
          else safeStorageRemove(RAIL_MODEL_LS);
        }}
      />

      <ChatThread bubbles={bubbles} isStreaming={isStreaming} />

      {error && (
        <div className="px-4 py-2 border-t border-border text-danger text-[12px]">
          {error}
        </div>
      )}

      <QuickRail
        scope={scope}
        disabled={isStreaming || !sessionId}
        onPick={(s) => {
          setInput(s);
          void send(s);
        }}
      />

      <ChatComposer
        value={input}
        placeholder={placeholder(scope)}
        onChange={setInput}
        onSubmit={() => void send(input)}
        disabled={!sessionId}
        busy={isStreaming}
        onCancel={stopStreaming}
        onOpenActions={onOpenActions}
      />
    </aside>
  );
}

function RailModelBar({
  rows,
  loading,
  provider,
  model,
  onChange,
}: {
  rows: ProviderRow[];
  loading: boolean;
  provider: string | null;
  model: string;
  onChange: (provider: string | null, model: string) => void;
}) {
  return (
    <div className="border-b border-border-soft px-4 py-2 bg-surface-2/30 flex items-center gap-2">
      <label className="text-[11px] text-text-3 uppercase tracking-wider">
        Model
      </label>
      <ModelPicker
        rows={rows}
        loading={loading}
        provider={provider}
        model={model}
        onChange={onChange}
        className="flex-1 min-w-0 text-[12px] bg-transparent border border-border-soft rounded-sm px-1.5 py-0.5 text-text font-mono"
        emptyHint="no models picked — visit Settings → Providers"
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// helpers — kept module-local to avoid spilling internals into the API layer.

/**
 * Map a successful wizard `tool_result` event to the TanStack Query keys
 * the tool just invalidated server-side, then call
 * `queryClient.invalidateQueries` for each so any mounted list query
 * refetches without a manual reload.
 *
 * Fixes `chat-rail-strategy-list-refresh`: today the chat rail mutates
 * server state via tool calls (`create_strategy`, `create_scenario`,
 * `update_slot`, …) but TanStack Query has no idea the cache went
 * stale. The operator only saw the new row after a hard refresh.
 *
 * No-op for non-tool events, for failed tool results, and for read-only
 * tools (`validate_draft`) — invalidating read-only tools would force a
 * pointless refetch.
 *
 * Tool → key map mirrors the wizard tool registry in
 * `crates/xvision-dashboard/src/wizard_loop.rs:446-541`. New tools that
 * mutate must be added here in the same PR they ship.
 */
export function invalidateForToolResult(qc: QueryClient, ev: WizardEvent): void {
  if (ev.type !== "tool_result") return;
  // Failed tool results don't mutate; nothing to invalidate. Require a
  // TRUTHY `error` value — checking only key presence used to bail on
  // legitimate success payloads that happened to ship `error: null` or
  // `error: ""` (common with Rust `Option<String>` serde defaults).
  // The wizard loop emits `{"error": "<msg>"}` on real failure, so a
  // truthiness check is enough to distinguish.
  const result = ev.result as
    | { error?: unknown; agent?: unknown }
    | null
    | undefined;
  if (result && typeof result === "object" && "error" in result && Boolean(result.error)) {
    return;
  }
  switch (ev.tool) {
    case "create_strategy":
    case "create_strategy_agent":
    case "attach_agent":
    case "update_slot":
    case "update_manifest":
    case "set_mechanical_param":
    case "set_risk_config":
      qc.invalidateQueries({ queryKey: strategyKeys.all });
      // `create_strategy_agent` always creates an agent row in the
      // library. `create_strategy` MAY also create a default agent —
      // when the wizard has a provider/model selected, the backend
      // calls `create_default_strategy_agent` and returns the new
      // agent under an `agent` key (see
      // crates/xvision-dashboard/src/wizard_loop.rs:467). When that
      // happens the agents list is stale until refetched.
      if (
        ev.tool === "create_strategy_agent" ||
        (ev.tool === "create_strategy" &&
          result &&
          typeof result === "object" &&
          result.agent != null)
      ) {
        qc.invalidateQueries({ queryKey: agentKeys.all });
      }
      return;
    case "create_scenario":
      qc.invalidateQueries({ queryKey: scenarioKeys.all });
      return;
    case "run_eval":
      qc.invalidateQueries({ queryKey: evalKeys.all });
      return;
    // Read-only — no invalidation.
    case "validate_draft":
      return;
    default:
      // Unknown tool: be conservative and skip. New mutating tools must
      // opt in explicitly so we don't spam refetches for every read.
      return;
  }
}

function applyEvent(
  setBubbles: Dispatch<SetStateAction<Bubble[]>>,
  ev: WizardEvent,
) {
  setBubbles((prev) => {
    const next = [...prev];
    const last = next[next.length - 1];
    if (!last || last.role !== "assistant") return next;
    const a = { ...last } as AssistantBubble;
    a.blocks = [...a.blocks];
    a.tools = [...a.tools];
    if (ev.type === "token") {
      appendAssistantText(a, ev.text);
    } else if (ev.type === "tool_call") {
      a.tools.push({
        call: ev.tool,
        ok: true,
        summary: summarizeArgs(ev.tool, ev.args),
        pending: true,
        args: ev.args,
      });
    } else if (ev.type === "tool_result") {
      let slot = -1;
      for (let i = a.tools.length - 1; i >= 0; i--) {
        if (a.tools[i].call === ev.tool) {
          slot = i;
          break;
        }
      }
      const result = ev.result as { error?: string };
      if (slot >= 0) {
        a.tools[slot] = {
          ...a.tools[slot],
          ok: !result?.error,
          summary: summarizeResult(ev.tool, ev.result),
          resultSummary: summarizeResult(ev.tool, ev.result),
          pending: false,
          result: ev.result,
        };
      }
    } else if (ev.type === "content_block") {
      a.blocks.push(contentBlockToRenderable(ev.block));
    } else if (ev.type === "error") {
      appendAssistantText(a, `\n\n[stream error: ${ev.message}]`);
    }
    next[next.length - 1] = a;
    return next;
  });
}

function historyToBubbles(history: ChatMessage[]): Bubble[] {
  const out: Bubble[] = [];
  let pendingAssistant: AssistantBubble | null = null;

  // First pass: collect assistant text + tool_use blocks per message.
  // Then attach matching tool_results from subsequent user messages onto
  // the prior assistant bubble's tool list.
  for (const cm of history) {
    if (cm.role === "user") {
      if (pendingAssistant) {
        out.push(pendingAssistant);
        pendingAssistant = null;
      }
      // A user turn carrying tool_result blocks updates the prior
      // assistant's tool chips; a plain text user turn becomes its own
      // bubble.
      const toolResults = cm.content_blocks.filter(
        (b): b is Extract<ContentBlock, { type: "tool_result" }> =>
          b.type === "tool_result",
      );
      if (toolResults.length > 0 && out.length > 0) {
        const prior = out[out.length - 1];
        if (prior.role === "assistant") {
          for (const tr of toolResults) {
            // Tool result content is the JSON-stringified result; surface
            // an error line if it parses to {error: ...}.
            // We don't know which tool_use this corresponds to without
            // the assistant's tool_use id; fall back to flipping the
            // most recent unresolved tool chip.
            if (prior.tools.length > 0) {
              const tool = prior.tools[prior.tools.length - 1];
              const parsedResult = safeParseJson(tr.content);
              const isErr =
                parsedResult &&
                typeof parsedResult === "object" &&
                parsedResult !== null &&
                "error" in parsedResult;
              prior.tools[prior.tools.length - 1] = {
                ...tool,
                ok: !isErr,
                summary: summarizeArgs(tool.call, tool.args),
                resultSummary: summarizeResult(tool.call, parsedResult),
                result: parsedResult ?? undefined,
              };
            }
          }
        }
      } else {
        const text = cm.content_blocks
          .filter((b): b is Extract<ContentBlock, { type: "text" }> =>
            b.type === "text",
          )
          .map((b) => b.text)
          .join("");
        if (text) out.push({ role: "user", text });
      }
    } else {
      // assistant
      const blocks = cm.content_blocks
        .filter((b) => b.type !== "tool_use" && b.type !== "tool_result")
        .map(contentBlockToRenderable);
      const tools: Tool[] = cm.content_blocks
        .filter((b): b is Extract<ContentBlock, { type: "tool_use" }> =>
          b.type === "tool_use",
        )
        .map((b) => ({
          call: b.name,
          ok: true,
          summary: summarizeArgs(b.name, b.input),
          args: b.input,
        }));
      pendingAssistant = { role: "assistant", blocks, tools };
    }
  }
  if (pendingAssistant) out.push(pendingAssistant);
  return out;
}

function contentBlockToRenderable(block: ContentBlock): RenderableBlock {
  if (block.type === "text") return { kind: "text", text: block.text };
  if (isRichDisplayBlock(block)) return { kind: "rich", block };
  return {
    kind: "unsupported",
    type: String((block as { type?: string }).type ?? "unknown"),
  };
}

function isRichDisplayBlock(block: ContentBlock): block is RichDisplayBlock {
  return (
    block.type === "inline_chart" ||
    block.type === "run_list" ||
    block.type === "strategy_card" ||
    block.type === "action_card" ||
    block.type === "choice_chips"
  );
}

function appendAssistantText(bubble: AssistantBubble, text: string) {
  const last = bubble.blocks[bubble.blocks.length - 1];
  if (last?.kind === "text") {
    bubble.blocks[bubble.blocks.length - 1] = {
      ...last,
      text: last.text + text,
    };
    return;
  }
  bubble.blocks.push({ kind: "text", text });
}

function safeParseJson(s: string): unknown {
  try {
    return JSON.parse(s);
  } catch {
    return null;
  }
}

function summarizeArgs(tool: string, args: unknown): string {
  const a = args as Record<string, unknown> | null | undefined;
  if (!a) return "";
  switch (tool) {
    case "create_strategy":
      return `${a["template"]} → ${a["name"]}`;
    case "update_slot":
      return String(a["slot"] ?? "");
    case "update_manifest": {
      const bits: string[] = [];
      if (Array.isArray(a["asset_universe"])) {
        bits.push(`assets=${(a["asset_universe"] as unknown[]).join(",")}`);
      }
      if (a["decision_cadence_minutes"]) {
        bits.push(`cadence=${a["decision_cadence_minutes"]}m`);
      }
      return bits.join("; ");
    }
    case "set_mechanical_param":
      return `${a["key"]} = ${JSON.stringify(a["value"])}`;
    case "set_risk_config":
      return a["preset"] ? `preset=${a["preset"]}` : "explicit";
    case "create_strategy_agent":
      return `${a["role"] ?? "trader"} · ${a["provider"] ?? "selected provider"} / ${a["model"] ?? "selected model"}`;
    case "attach_agent":
      return `${a["agent_id"] ?? ""} as ${a["role"] ?? "trader"}`;
    case "get_strategy":
    case "validate_draft":
      return String(a["id"] ?? "");
    case "list_templates":
      return "all";
    default:
      return "";
  }
}

function summarizeResult(tool: string, result: unknown): string {
  const r = result as Record<string, unknown> | null;
  if (!r) return "";
  if (r.error) return String(r.error);
  switch (tool) {
    case "list_templates":
      return Array.isArray(result)
        ? `${(result as unknown[]).length} templates`
        : "";
    case "create_strategy":
      return r.id ? String(r.id) : "";
    case "create_strategy_agent":
      return r.agent_id ? String(r.agent_id) : "";
    case "attach_agent":
      return Array.isArray(r.agents)
        ? `${(r.agents as unknown[]).length} agent(s)`
        : "";
    case "validate_draft":
      return r.ok
        ? "ok"
        : `${(r.errors as string[] | undefined)?.length ?? 0} error(s)`;
    case "update_slot":
    case "update_manifest":
      return Array.isArray(r.updated) ? (r.updated as string[]).join(", ") : "";
    case "set_risk_config":
      return r.applied ? String(r.applied) : "";
    default:
      return "";
  }
}

function formatErr(e: unknown): string {
  if (e instanceof ApiError) return `${e.code}: ${e.message}`;
  return String(e);
}
