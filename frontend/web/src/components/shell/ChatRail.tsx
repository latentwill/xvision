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
} from "react";
import { useLocation } from "react-router-dom";

import { Icon } from "@/components/primitives/Icon";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import {
  type ChatMessage,
  type ContentBlock,
  type ContextScope,
  type WizardEvent,
  createSession,
  deleteSession,
  fetchHistory,
  headerLabel,
  placeholder,
  quickReplies,
  scopeFromPath,
  scopeKey,
  streamChat,
  updateScope,
} from "@/api/chat_rail";

const SESSION_LS_PREFIX = "xvn.chat_rail.session.";
const RAIL_OPEN_LS = "xvn.chat_rail.open";

type Tool = { call: string; ok: boolean; summary: string };
type AssistantBubble = {
  role: "assistant";
  text: string;
  tools: Tool[];
};
type UserBubble = { role: "user"; text: string };
type Bubble = UserBubble | AssistantBubble;

export function ChatRail() {
  const location = useLocation();
  const scope = useMemo<ContextScope>(
    () => scopeFromPath(location.pathname),
    [location.pathname],
  );
  const key = useMemo(() => scopeKey(scope), [scope]);

  const [open, setOpen] = useState<boolean>(() => {
    return localStorage.getItem(RAIL_OPEN_LS) === "1";
  });
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [bubbles, setBubbles] = useState<Bubble[]>([]);
  const [input, setInput] = useState("");
  const [isStreaming, setIsStreaming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const lastScopeKeyRef = useRef<string | null>(null);

  // Persist open/close so the rail stays in the user's chosen state across
  // route changes (and reloads).
  useEffect(() => {
    localStorage.setItem(RAIL_OPEN_LS, open ? "1" : "0");
  }, [open]);

  // When the rail is open and the scope changes, ensure we have a session
  // for the current scope-key + load its history. Cached session id lives
  // in localStorage keyed by scope so the conversation resumes.
  useEffect(() => {
    if (!open) return;
    if (lastScopeKeyRef.current === key && sessionId) return;
    lastScopeKeyRef.current = key;

    let cancelled = false;
    (async () => {
      setError(null);
      try {
        const cached = localStorage.getItem(SESSION_LS_PREFIX + key);
        let id: string;
        if (cached) {
          id = cached;
          // Make sure server-side scope matches (handles the case where
          // the user changed pages between sessions but the cached id
          // was created with a different scope).
          await updateScope(id, scope).catch(() => {
            // 404 → session was deleted server-side; fall back to fresh.
            throw new Error("session-stale");
          });
        } else {
          id = await createSession(scope);
          localStorage.setItem(SESSION_LS_PREFIX + key, id);
        }
        if (cancelled) return;
        setSessionId(id);
        const history = await fetchHistory(id);
        if (cancelled) return;
        setBubbles(historyToBubbles(history));
      } catch (e) {
        if (cancelled) return;
        if ((e as Error).message === "session-stale") {
          localStorage.removeItem(SESSION_LS_PREFIX + key);
          try {
            const id = await createSession(scope);
            localStorage.setItem(SESSION_LS_PREFIX + key, id);
            setSessionId(id);
            setBubbles([]);
          } catch (e2) {
            setError(formatErr(e2));
          }
          return;
        }
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
        { role: "assistant", text: "", tools: [] },
      ]);
      setIsStreaming(true);
      const ctrl = new AbortController();
      abortRef.current = ctrl;
      try {
        for await (const ev of streamChat(
          { session_id: sessionId, message: userText },
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
    [sessionId, isStreaming],
  );

  const startFresh = useCallback(async () => {
    if (!sessionId) return;
    abortRef.current?.abort();
    try {
      await deleteSession(sessionId);
    } catch {
      /* best-effort */
    }
    localStorage.removeItem(SESSION_LS_PREFIX + key);
    setSessionId(null);
    setBubbles([]);
    setError(null);
    // Force the open-effect to re-create a session for this scope.
    lastScopeKeyRef.current = null;
  }, [sessionId, key]);

  if (!open) {
    return (
      <aside
        className="hidden xl:flex w-[44px] flex-col items-center gap-3 border-l border-border-soft bg-surface-sidebar py-4"
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
      className="hidden xl:flex w-[360px] flex-col border-l border-border-soft bg-surface-sidebar"
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
            Start fresh
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

      <Thread bubbles={bubbles} />

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

function Thread({ bubbles }: { bubbles: Bubble[] }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    ref.current?.scrollTo({
      top: ref.current.scrollHeight,
      behavior: "smooth",
    });
  }, [bubbles]);
  return (
    <div
      ref={ref}
      className="flex-1 overflow-y-auto px-4 py-3 flex flex-col gap-2"
    >
      {bubbles.length === 0 ? (
        <div className="text-text-3 italic text-[13px] text-center py-4">
          No messages yet. Ask the agent something — it has tools for the
          authoring loop.
        </div>
      ) : (
        bubbles.map((b, i) => <BubbleView key={i} b={b} />)
      )}
    </div>
  );
}

function BubbleView({ b }: { b: Bubble }) {
  if (b.role === "user") {
    return (
      <div className="self-end max-w-[92%]">
        <div className="bg-blue-500/10 dark:bg-blue-400/10 border border-blue-500/30 dark:border-blue-400/30 rounded-md px-2.5 py-1.5 text-[13px] whitespace-pre-wrap leading-snug">
          {b.text}
        </div>
      </div>
    );
  }
  return (
    <div className="self-start max-w-[92%]">
      <div className="bg-surface-2/60 border border-border rounded-md px-2.5 py-1.5 text-[13px] whitespace-pre-wrap leading-snug">
        {b.text || <span className="text-text-3 italic">thinking…</span>}
      </div>
      {b.tools.length > 0 && (
        <div className="mt-1.5 flex flex-wrap gap-1">
          {b.tools.map((t, i) => (
            <Pill key={i} tone={t.ok ? "info" : "danger"}>
              <span className="font-mono">{t.call}</span>
              {t.summary && (
                <span className="text-text-3"> · {t.summary}</span>
              )}
            </Pill>
          ))}
        </div>
      )}
    </div>
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
    a.tools = [...a.tools];
    if (ev.type === "token") {
      a.text = a.text + ev.text;
    } else if (ev.type === "tool_call") {
      a.tools.push({
        call: ev.tool,
        ok: true,
        summary: summarizeArgs(ev.tool, ev.args),
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
        };
      }
    } else if (ev.type === "error") {
      a.text = a.text
        ? `${a.text}\n\n[stream error: ${ev.message}]`
        : `[stream error: ${ev.message}]`;
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
            // an error chip if it parses to {error: ...}.
            const parsed = safeParseJson(tr.content);
            const isErr =
              parsed &&
              typeof parsed === "object" &&
              parsed !== null &&
              "error" in parsed;
            // We don't know which tool_use this corresponds to without
            // the assistant's tool_use id; fall back to flipping the
            // most recent unresolved tool chip.
            if (prior.tools.length > 0) {
              const tool = prior.tools[prior.tools.length - 1];
              prior.tools[prior.tools.length - 1] = {
                ...tool,
                ok: !isErr,
                summary: tool.summary,
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
      const text = cm.content_blocks
        .filter((b): b is Extract<ContentBlock, { type: "text" }> =>
          b.type === "text",
        )
        .map((b) => b.text)
        .join("");
      const tools: Tool[] = cm.content_blocks
        .filter((b): b is Extract<ContentBlock, { type: "tool_use" }> =>
          b.type === "tool_use",
        )
        .map((b) => ({
          call: b.name,
          ok: true,
          summary: summarizeArgs(b.name, b.input),
        }));
      pendingAssistant = { role: "assistant", text, tools };
    }
  }
  if (pendingAssistant) out.push(pendingAssistant);
  return out;
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

function formatErr(e: unknown): string {
  if (e instanceof ApiError) return `${e.code}: ${e.message}`;
  return String(e);
}
