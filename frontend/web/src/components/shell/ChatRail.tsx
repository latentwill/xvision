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
  type ReactNode,
} from "react";
import { useLocation } from "react-router-dom";

import { useQuery } from "@tanstack/react-query";

import { MarkdownView } from "@/components/agent-chat/MarkdownView";
import {
  ContentBlockView,
  type RichDisplayBlock,
} from "@/components/chat/ContentBlockView";
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
  quickReplies,
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

type Tool = {
  call: string;
  ok: boolean;
  summary: string;
  resultSummary?: string;
  /** True between tool_call and tool_result; drives the chip spinner. */
  pending?: boolean;
  /** Raw args from tool_call; consumed by toolNarrative for inline confirmations. */
  args?: unknown;
  /** Raw result from tool_result; consumed by toolNarrative for inline confirmations. */
  result?: unknown;
};
type AssistantBubble = {
  role: "assistant";
  blocks: RenderableBlock[];
  tools: Tool[];
};
type UserBubble = { role: "user"; text: string };
type Bubble = UserBubble | AssistantBubble;
type RenderableBlock =
  | { kind: "text"; text: string }
  | { kind: "rich"; block: RichDisplayBlock }
  | { kind: "unsupported"; type: string };

export function ChatRail() {
  const location = useLocation();
  const scope = useMemo<ContextScope>(
    () => scopeFromPath(location.pathname),
    [location.pathname],
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
    enabled: open,
  });
  const sessionsQ = useQuery({
    queryKey: ["chat-rail", "sessions"],
    queryFn: listSessions,
    enabled: open,
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
    safeStorageSet(RAIL_OPEN_LS, open ? "1" : "0");
  }, [open]);

  // When the rail is open and the scope changes, resolve a session for
  // the current scope. The server owns session lifecycle — the rail
  // never holds a stale id across DB resets or fresh deploys.
  useEffect(() => {
    if (!open) return;
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
  }, [open, key, scope, sessionId]);

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
        }
      } catch (e) {
        if ((e as Error).name === "AbortError") return;
        setError(formatErr(e));
      } finally {
        setIsStreaming(false);
        abortRef.current = null;
      }
    },
    [sessionId, isStreaming, providerName, modelId],
  );

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

  if (!open) {
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
      className="hidden xl:flex w-[360px] flex-col h-screen sticky top-0 border-l border-border-soft bg-surface-sidebar"
      aria-label="Chat rail"
    >
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
          <button
            className="text-text-3 hover:text-text"
            onClick={() => setOpen(false)}
            title="Collapse rail"
          >
            <Icon name="chevR" size={14} />
          </button>
        </div>
      </header>
      {recentScopeSessions.length > 0 && (
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

      <Thread bubbles={bubbles} isStreaming={isStreaming} />

      {error && (
        <div className="px-4 py-2 border-t border-border text-rose-300 text-[12px]">
          {error}
        </div>
      )}

      <QuickReplies
        scope={scope}
        disabled={isStreaming || !sessionId}
        onPick={(s) => {
          setInput(s);
          void send(s);
        }}
      />

      <Composer
        value={input}
        placeholder={placeholder(scope)}
        onChange={setInput}
        onSubmit={() => void send(input)}
        disabled={isStreaming || !sessionId}
      />
    </aside>
  );
}

function Thread({
  bubbles,
  isStreaming,
}: {
  bubbles: Bubble[];
  isStreaming: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!ref.current || typeof ref.current.scrollTo !== "function") return;
    ref.current.scrollTo({
      top: ref.current.scrollHeight,
      behavior: "smooth",
    });
  }, [bubbles]);
  return (
    <div
      ref={ref}
      className="flex-1 min-h-0 overflow-y-auto px-4 py-3 flex flex-col gap-2"
    >
      {bubbles.length === 0 ? (
        <div className="text-text-3 italic text-[13px] text-center py-4">
          No messages yet. Ask the agent something — it has tools for the
          authoring loop.
        </div>
      ) : (
        bubbles.map((b, i) => (
          <BubbleView
            key={i}
            b={b}
            isLast={i === bubbles.length - 1}
            isStreaming={isStreaming}
          />
        ))
      )}
    </div>
  );
}

function BubbleView({
  b,
  isLast,
  isStreaming,
}: {
  b: Bubble;
  isLast: boolean;
  isStreaming: boolean;
}) {
  if (b.role === "user") {
    return (
      <div className="self-end max-w-[92%]">
        <div className="bg-blue-500/10 dark:bg-blue-400/10 border border-blue-500/30 dark:border-blue-400/30 rounded-md px-2.5 py-1.5 text-[13px] whitespace-pre-wrap leading-snug">
          {b.text}
        </div>
      </div>
    );
  }
  const showDots = isStreaming && isLast;
  const hasRenderableBlocks = b.blocks.some(
    (block) => block.kind !== "text" || block.text.length > 0,
  );
  const logs = b.tools
    .map((t) => ({ n: toolLogLine(t) }))
    .filter(
      (x): x is { n: { ok: boolean; content: ReactNode } } =>
        x.n !== null,
    );
  return (
    <div className="self-start max-w-[92%]">
      <div className="bg-surface-2/60 border border-border rounded-md px-2.5 py-1.5 text-[13px] leading-snug">
        {hasRenderableBlocks ? (
          <>
            <ContentBlocksView blocks={b.blocks} />
            {showDots && <TypingDots inline />}
          </>
        ) : showDots ? (
          <TypingDots />
        ) : (
          <span className="text-text-3 italic">thinking…</span>
        )}
      </div>
      {logs.length > 0 && (
        <div className="mt-1.5 flex flex-col gap-1">
          {logs.map(({ n }, i) => (
            <div
              key={`narr-${i}`}
              className={`text-[12px] flex items-start gap-1.5 ${
                n.ok ? "text-emerald-300" : "text-rose-300"
              }`}
            >
              <span className="leading-[1.4] flex-shrink-0">
                {n.ok ? "✓" : "✗"}
              </span>
              <span className="leading-[1.4]">{n.content}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function ContentBlocksView({ blocks }: { blocks: RenderableBlock[] }) {
  return (
    <div className="flex flex-col gap-2">
      {blocks.map((block, index) => {
        if (block.kind === "text") {
          if (!block.text) return null;
          return <MarkdownView key={index} text={block.text} />;
        }
        if (block.kind === "rich") {
          return <UnsupportedRichBlock key={index} block={block.block} />;
        }
        return <UnsupportedBlockNotice key={index} type={block.type} />;
      })}
    </div>
  );
}

function UnsupportedRichBlock({ block }: { block: RichDisplayBlock }) {
  return <ContentBlockView block={block} />;
}

function UnsupportedBlockNotice({ type }: { type: string }) {
  return (
    <div className="rounded border border-border-soft bg-surface-elev px-2 py-1 text-[12px] text-text-3">
      Unsupported chat block: <span className="font-mono">{type}</span>
    </div>
  );
}

function TypingDots({ inline }: { inline?: boolean }) {
  return (
    <span
      className={`inline-flex items-center gap-1 align-middle ${inline ? "ml-1.5" : ""}`}
      aria-label="generating"
    >
      <span
        className="w-1.5 h-1.5 rounded-full bg-text-3 animate-pulse"
        style={{ animationDelay: "0ms" }}
      />
      <span
        className="w-1.5 h-1.5 rounded-full bg-text-3 animate-pulse"
        style={{ animationDelay: "150ms" }}
      />
      <span
        className="w-1.5 h-1.5 rounded-full bg-text-3 animate-pulse"
        style={{ animationDelay: "300ms" }}
      />
    </span>
  );
}

function QuickReplies({
  scope,
  disabled,
  onPick,
}: {
  scope: ContextScope;
  disabled: boolean;
  onPick: (s: string) => void;
}) {
  const replies = quickReplies(scope);
  if (replies.length === 0) return null;
  return (
    <div className="border-t border-border-soft px-3 py-2 flex flex-wrap gap-1">
      {replies.map((r) => (
        <button
          key={r}
          disabled={disabled}
          onClick={() => onPick(r)}
          className="text-[11px] text-text-2 hover:text-text border border-border-soft rounded-full px-2.5 py-1 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {r}
        </button>
      ))}
    </div>
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

function Composer({
  value,
  placeholder,
  onChange,
  onSubmit,
  disabled,
}: {
  value: string;
  placeholder: string;
  onChange: (s: string) => void;
  onSubmit: () => void;
  disabled: boolean;
}) {
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit();
      }}
      className="border-t border-border-soft px-3 py-2.5 flex gap-2 bg-surface-2/30"
    >
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={disabled}
        placeholder={placeholder}
        className="flex-1 bg-transparent border border-border-soft rounded-md px-2.5 py-1.5 text-[13px] placeholder:text-text-3 focus:outline-none focus:ring-1 focus:ring-text-2"
      />
      <button
        type="submit"
        disabled={disabled || !value.trim()}
        className="px-2.5 py-1.5 rounded-md text-[12px] border border-border-soft bg-surface-2/60 hover:bg-surface-2 disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {disabled ? "…" : "Send"}
      </button>
    </form>
  );
}

// ---------------------------------------------------------------------------
// helpers — kept module-local to avoid spilling internals into the API layer.

function applyEvent(
  setBubbles: React.Dispatch<React.SetStateAction<Bubble[]>>,
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
    case "set_mechanical_param":
      return `${a["key"]} = ${JSON.stringify(a["value"])}`;
    case "set_risk_config":
      return a["preset"] ? `preset=${a["preset"]}` : "explicit";
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
    case "validate_draft":
      return r.ok
        ? "ok"
        : `${(r.errors as string[] | undefined)?.length ?? 0} error(s)`;
    case "update_slot":
      return Array.isArray(r.updated) ? (r.updated as string[]).join(", ") : "";
    case "set_risk_config":
      return r.applied ? String(r.applied) : "";
    default:
      return "";
  }
}

// ---------------------------------------------------------------------------
// Tool transcript lines — explicit in-thread call/result logging that
// shows each xvn invocation and final result.

function toolLogLine(
  t: Tool,
): { ok: boolean; content: ReactNode } | null {
  const args = (t.args ?? {}) as Record<string, unknown>;
  const result = (t.result ?? {}) as Record<string, unknown>;
  const errorMsg =
    typeof result.error === "string" ? result.error : undefined;
  if (errorMsg) {
    return {
      ok: false,
      content: (
        <>
          {friendlyVerb(t.call)} failed: <span>{errorMsg}</span>
        </>
      ),
    };
  }
  switch (t.call) {
    case "get_strategy":
    case "list_templates": {
      const arg = args["template"] ?? args["id"] ?? "all";
      return {
        ok: true,
        content: t.pending ? (
          <>
            Calling <code className="font-mono text-text">{t.call}</code> with{" "}
            <code className="font-mono text-text-2">{String(arg)}</code> …
          </>
        ) : (
          <>
            {t.call} returned{" "}
            <span className="font-mono text-text">{t.resultSummary ?? ""}</span>
          </>
        ),
      };
    }
    case "create_strategy": {
      const name = String(args["name"] ?? "(unnamed)");
      const template = String(args["template"] ?? "");
      const id = typeof result["id"] === "string" ? result["id"] : "";
      return {
        ok: true,
        content: t.pending ? (
          <>
            Calling <code className="font-mono text-text">create_strategy</code>{" "}
            for <strong className="text-text font-semibold">{name}</strong>
            {template && (
              <>
                {" "}from{" "}
                <code className="font-mono text-text-2">{template}</code>
              </>
            )}
          </>
        ) : (
          <>
            Created strategy{" "}
            <strong className="text-text font-semibold">{name}</strong>
            {template && (
              <>
                {" "}from{" "}
                <code className="font-mono text-text">{template}</code>
              </>
            )}
            {id && (
              <>
                {" "}(<code className="font-mono text-text-2">{id}</code>)
              </>
            )}
          </>
        ),
      };
    }
    case "set_mechanical_param": {
      const key = String(args["key"] ?? "?");
      const rawValue = args["value"];
      const value =
        rawValue === undefined
          ? "?"
          : typeof rawValue === "string"
            ? rawValue
            : JSON.stringify(rawValue);
      return {
        ok: true,
        content: (
          <>
            {t.pending ? "Calling" : "Set"} {" "}
            <code className="font-mono text-text">{key}</code> ={" "}
            <code className="font-mono text-text">{value}</code>
          </>
        ),
      };
    }
    case "set_risk_config": {
      const preset =
        typeof args["preset"] === "string"
          ? (args["preset"] as string)
          : undefined;
      return {
        ok: true,
        content: preset ? (
          <>
            Risk preset:{" "}
            <strong className="text-text font-semibold">{preset}</strong>
          </>
        ) : (
          <>Risk: explicit settings applied</>
        ),
      };
    }
    case "validate_draft": {
      const ok = result["ok"] === true;
      const errs = Array.isArray(result["errors"])
        ? (result["errors"] as unknown[]).length
        : 0;
      return ok
        ? { ok: true, content: <>Validation passed</> }
        : {
            ok: false,
            content: (
              <>
                Validation failed ({errs} error{errs === 1 ? "" : "s"})
              </>
            ),
          };
    }
    case "update_slot": {
      const slot = String(args["slot"] ?? "?");
      const updated = Array.isArray(result["updated"])
        ? (result["updated"] as string[]).join(", ")
        : "";
      return {
        ok: true,
        content: t.pending ? (
          <>
            Updating <code className="font-mono text-text">{slot}</code>…
          </>
        ) : updated ? (
          <>
            Updated <code className="font-mono text-text">{slot}</code>:{" "}
            {updated}
          </>
        ) : (
          <>
            Updated <code className="font-mono text-text">{slot}</code>
          </>
        ),
      };
    }
    case "run_eval": {
      if (t.pending) {
        return {
          ok: true,
          content: (
            <>
              Running <code className="font-mono text-text">eval</code>...
            </>
          ),
        };
      }
      const runId =
        typeof result["run_id"] === "string"
          ? (result["run_id"] as string)
          : "";
      return {
        ok: true,
        content: runId ? (
          <>
            Eval run{" "}
            <code className="font-mono text-text">{runId}</code>{" "}
            {t.resultSummary ? (
              <span className="text-text-2">({t.resultSummary})</span>
            ) : null}
          </>
        ) : (
          <>Eval action complete</>
        ),
      };
    }
    default:
      if (t.pending) {
        return {
          ok: true,
          content: (
            <>
              Calling <code className="font-mono text-text">{t.call}</code>{" "}
              <span className="text-text-2">
                {summarizeArgs(t.call, t.args)}
              </span>
              …
            </>
          ),
        };
      }
      return {
        ok: true,
        content: (
          <>
            {t.call} completed
            {t.resultSummary ? (
              <span className="text-text-2">: {t.resultSummary}</span>
            ) : null}
          </>
        ),
      };
  }
}

function friendlyVerb(call: string): string {
  switch (call) {
    case "create_strategy":
      return "Create strategy";
    case "set_mechanical_param":
      return "Set parameter";
    case "set_risk_config":
      return "Set risk";
    case "validate_draft":
      return "Validate";
    case "update_slot":
      return "Update slot";
    default:
      return call;
  }
}

function formatErr(e: unknown): string {
  if (e instanceof ApiError) return `${e.code}: ${e.message}`;
  return String(e);
}
