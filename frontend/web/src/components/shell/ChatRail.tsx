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
import { ChatHistoryItem } from "@/components/chat/ChatHistoryItem";
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
import { useUi } from "@/stores/ui";
import {
  type ChatMessage,
  type ChatSessionMode,
  type ContentBlock,
  type ContextScope,
  type WizardEvent,
  UNIFIED_STREAM_REPLAY_FROM_START,
  createSession,
  listSessions,
  loadSessionHistory,
  openUnifiedSessionStream,
  placeholder,
  resolveSession,
  scopeFromPath,
  scopeKey,
  setSessionMode,
  streamChat,
} from "@/api/chat_rail";
import { listProviders, settingsKeys } from "@/api/settings";
import { isProviderConfigured } from "@/lib/providers";
import type { ProviderRow } from "@/api/types.gen";
import {
  useSessionEvents,
  useSessionRows,
} from "@/stores/session-events";
import { useTraceDock } from "@/stores/trace-dock";
import type {
  MessageRow,
  ToolRow,
} from "@/stores/message-row-reducer";

const RAIL_OPEN_LS = "xvn.chat_rail.open";
const RAIL_PROVIDER_LS = "xvn.chat_rail.provider";
const RAIL_MODEL_LS = "xvn.chat_rail.model";
const RAIL_MODE_LS = "xvn.chat_rail.mode";
const RAIL_HISTORY_COLLAPSED_LS = "xvn.chat_rail.history_collapsed";
const RAIL_CONTEXT_MODE_LS = "xvn.chat_rail.context_mode";

// Backoff ladder for the resolveSession self-heal. Mirrors the unified-SSE
// reconnect schedule in `api/chat_rail.ts`. resolveSession runs in a one-shot
// effect with no query-style retry, so a transient failure (e.g. a `502`
// during a backend deploy/restart window, where `tailscale serve` has no
// upstream for a few seconds) would otherwise leave the rail sessionless —
// no session id → no stream → dead rail — until a manual page refresh. The
// last entry repeats, so recovery keeps trying indefinitely (never gives up).
const RESOLVE_BACKOFF_MS = [500, 1000, 2000, 4000, 8000];

function readPersistedMode(): ChatSessionMode {
  const v = safeStorageGet(RAIL_MODE_LS);
  return v === "act" ? "act" : "research";
}

export type RailContextMode = "active" | "workspace";

function readPersistedContextMode(): RailContextMode {
  const v = safeStorageGet(RAIL_CONTEXT_MODE_LS);
  return v === "workspace" ? "workspace" : "active";
}

const CONTEXT_MODE_LABEL: Record<RailContextMode, string> = {
  active: "Active page",
  workspace: "Whole workspace",
};

function shortModelName(model: string): string {
  const lastSlash = model.lastIndexOf("/");
  return lastSlash >= 0 ? model.slice(lastSlash + 1) : model;
}

function describeRailModelSource(
  rows: ProviderRow[],
  defaultModel: string | null,
  provider: string | null,
  model: string,
): string {
  if (!provider || !model) return "No chat model selected.";
  const row = rows.find((r) => r.name === provider);
  const short = shortModelName(model);
  if (row?.is_default && defaultModel === model) {
    return `Workspace default: ${provider} / ${short}`;
  }
  if (row?.is_default && !defaultModel) {
    return `Workspace default: ${provider}`;
  }
  return `${provider} / ${short}`;
}

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
  const [contextMode, setContextMode] = useState<RailContextMode>(
    () => readPersistedContextMode(),
  );
  const [contextMenuOpen, setContextMenuOpen] = useState(false);
  const scope = useMemo<ContextScope>(
    () =>
      contextMode === "workspace"
        ? { scope: "workspace" }
        : scopeFromPath(location.pathname, location.search),
    [contextMode, location.pathname, location.search],
  );
  const key = useMemo(() => scopeKey(scope), [scope]);

  const selectContextMode = useCallback((next: RailContextMode) => {
    setContextMode(next);
    safeStorageSet(RAIL_CONTEXT_MODE_LS, next);
    setContextMenuOpen(false);
  }, []);

  const [open, setOpen] = useState<boolean>(() => {
    return safeStorageGet(RAIL_OPEN_LS) === "1";
  });
  const chatRailWidth = useUi((s) => s.chatRailWidth);
  const setChatRailOpen = useUi((s) => s.setChatRailOpen);
  const setOpenAndSync = useCallback(
    (v: boolean) => {
      safeStorageSet(RAIL_OPEN_LS, v ? "1" : "0");
      setOpen(v);
      setChatRailOpen(v);
    },
    [setChatRailOpen],
  );
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [bubbles, setBubbles] = useState<Bubble[]>([]);
  const [input, setInput] = useState("");
  const [isStreaming, setIsStreaming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<ChatSessionMode>(() => readPersistedMode());
  const [modePending, setModePending] = useState(false);
  const [historyCollapsed, setHistoryCollapsed] = useState<boolean>(() => {
    return safeStorageGet(RAIL_HISTORY_COLLAPSED_LS) === "1";
  });
  const [providerName, setProviderName] = useState<string | null>(
    () => safeStorageGet(RAIL_PROVIDER_LS),
  );
  const [modelId, setModelId] = useState<string>(
    () => safeStorageGet(RAIL_MODEL_LS) ?? "",
  );
  const abortRef = useRef<AbortController | null>(null);
  const sessionIdRef = useRef<string | null>(null);
  const lastScopeKeyRef = useRef<string | null>(null);
  // Single-flight gate for "session missing → resolve fresh session"
  // recovery. When two parallel send() calls both hit
  // `chat_session_missing` (rapid double-send during a workspace
  // reset, e.g.), the second one awaits the first's resolution
  // instead of triggering a duplicate resolveSession that would mint
  // two sessions and silently lose one's reply.
  const recoveringSessionRef = useRef<Promise<string | null> | null>(null);
  // resolveSession self-heal state. `resolveAttemptRef` indexes the backoff
  // ladder; `resolveRetryTimerRef` holds the pending retry timer so it can be
  // cancelled on scope change / unmount; bumping `resolveNonce` re-runs the
  // resolve effect (it's a dep) without otherwise touching scope/session.
  const resolveAttemptRef = useRef(0);
  const resolveRetryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const [resolveNonce, setResolveNonce] = useState(0);

  const providers = useQuery({
    queryKey: settingsKeys.providers(),
    queryFn: listProviders,
    enabled: variant === "panel" || open,
  });
  const railModelSourceLabel = useMemo(
    () =>
      describeRailModelSource(
        providers.data?.providers ?? [],
        providers.data?.default_model ?? null,
        providerName,
        modelId,
      ),
    [providers.data, providerName, modelId],
  );
  const sessionsQ = useQuery({
    queryKey: ["chat-rail", "sessions"],
    queryFn: listSessions,
    enabled: variant === "panel" || open,
    refetchInterval: 5000,
  });
  // Auto-pick the (provider, model) for the chat dispatch when the
  // catalog loads. The tiebreaker order is deliberate — pre-2026-05-26
  // the rail just took `candidates[0]`, which made the provider that
  // happened to be first in the catalog (in practice: OpenRouter with
  // deepseek-v4-pro) the silent default even when the operator had set
  // a different provider as the workspace default. That selection
  // would then cascade into every wizard-created agent through the
  // backend `resolve_agent_runtime` fallback (now removed), and the
  // assistant would synthesize a confusing "no Gemini models on
  // OpenRouter" response when the agent ran against the wrong route.
  //
  // Priority:
  //   1. The currently-selected (providerName, modelId) IF it still
  //      exists in the catalog and is enabled — that's the operator's
  //      most recent explicit choice (persisted via localStorage via
  //      `useState` initializer above, so this also covers reloads).
  //   2. If the selected provider exists but its model is no longer
  //      enabled, switch to that provider's first enabled model
  //      rather than swapping providers — the operator picked the
  //      provider deliberately.
  //   3. The workspace default provider (`is_default: true`) using
  //      `default_model` from ProvidersReport when present.
  //   4. First candidate with at least one enabled model (legacy
  //      behavior, kept only as a last resort).
  useEffect(() => {
    const data = providers.data;
    if (!data) return;
    const rows = data.providers ?? [];
    const candidates = rows.filter(
      (p) => isProviderConfigured(p) && p.enabled_models.length > 0,
    );

    let staleSelection = false;

    // (1) current selection still valid → no-op.
    if (providerName && modelId) {
      const cur = candidates.find((c) => c.name === providerName);
      if (cur && cur.enabled_models.includes(modelId)) return;
      // (2) provider valid, model no longer enabled → swap model only.
      if (cur && cur.enabled_models.length > 0) {
        const m = cur.enabled_models[0];
        setModelId(m);
        safeStorageSet(RAIL_MODEL_LS, m);
        return;
      }
      // Selection is fully stale (provider gone or disabled). Fall
      // through to default-then-first-candidate resolution.
      staleSelection = true;
    } else if (providerName || modelId) {
      staleSelection = true;
    }

    if (staleSelection) {
      setProviderName(null);
      setModelId("");
      safeStorageRemove(RAIL_PROVIDER_LS);
      safeStorageRemove(RAIL_MODEL_LS);
    }

    // (3) workspace default.
    const def = candidates.find((c) => c.is_default);
    const defaultModelOnDef =
      def && data.default_model && def.enabled_models.includes(data.default_model)
        ? data.default_model
        : def?.enabled_models[0];
    if (def && defaultModelOnDef) {
      setProviderName(def.name);
      setModelId(defaultModelOnDef);
      safeStorageSet(RAIL_PROVIDER_LS, def.name);
      safeStorageSet(RAIL_MODEL_LS, defaultModelOnDef);
      return;
    }

    // (4) first candidate fallback.
    const pick = candidates[0];
    if (!pick) return;
    const m = pick.enabled_models[0];
    setProviderName(pick.name);
    setModelId(m);
    safeStorageSet(RAIL_PROVIDER_LS, pick.name);
    safeStorageSet(RAIL_MODEL_LS, m);
  }, [providerName, modelId, providers.data]);

  // Persist open/close so the rail stays in the user's chosen state across
  // route changes (and reloads). Also sync to the shared UI store so the
  // shell can adjust its grid layout without the rail needing a prop channel.
  useEffect(() => {
    if (variant !== "desktop") return;
    safeStorageSet(RAIL_OPEN_LS, open ? "1" : "0");
    setChatRailOpen(open);
  }, [open, variant, setChatRailOpen]);

  // Persist mode so a new chat inherits the user's last choice (Think/Act)
  // instead of always defaulting back to "research".
  useEffect(() => {
    safeStorageSet(RAIL_MODE_LS, mode);
  }, [mode]);

  useEffect(() => {
    sessionIdRef.current = sessionId;
  }, [sessionId]);

  // ── Unified event stream (Phase 1.2/1.4) ────────────────────────────────
  // One stream → one event log → two projections (rail rows + trace dock).
  // When a session is bound and the rail is active, open the unified SSE
  // stream and ingest every UnifiedEvent into the shared session-events
  // store. Rail rows render from that store's `reduceRows` projection; the
  // trace dock reads the SAME store (via its session binding). Ingestion is
  // idempotent (dedupe by event_id) so reconnect/replay never duplicates.
  const ingest = useSessionEvents((s) => s.ingest);
  const resetSessionEvents = useSessionEvents((s) => s.reset);
  useEffect(() => {
    if (variant === "desktop" && !open) return;
    if (!sessionId) return;
    const boundSession = sessionId;
    // Bind the trace dock to this session so its span view projects from the
    // same unified log (one stream, two projections — Phase 1.2/1.4).
    useTraceDock.getState().setActiveSession(boundSession);
    const close = openUnifiedSessionStream(
      boundSession,
      UNIFIED_STREAM_REPLAY_FROM_START,
      {
        onEvent: (ev) => ingest(boundSession, ev),
      },
    );
    return () => {
      close();
      // Only clear the binding if it's still pointing at this session.
      if (useTraceDock.getState().activeSessionId === boundSession) {
        useTraceDock.getState().setActiveSession(null);
      }
    };
  }, [sessionId, open, variant, ingest]);

  // Rail-row projection of the unified log for the active session.
  const unifiedRows = useSessionRows(sessionId);

  const abortActiveStream = useCallback(() => {
    abortRef.current?.abort();
  }, []);

  // When the rail is open and the scope changes, resolve a session for
  // the current scope. The server owns session lifecycle — the rail
  // never holds a stale id across DB resets or fresh deploys.
  useEffect(() => {
    if (variant === "desktop" && !open) return;
    if (lastScopeKeyRef.current !== key) abortActiveStream();
    if (lastScopeKeyRef.current === key && sessionId) return;
    lastScopeKeyRef.current = key;

    let cancelled = false;
    (async () => {
      setError(null);
      try {
        const resolved = await resolveSession(scope);
        if (cancelled) return;
        // Recovered: if we'd been retrying a failed resolve (deploy window),
        // the sessions list and providers catalog may still be parked in an
        // error state — refetch them so the whole rail is usable again, not
        // just the freshly-resolved session.
        const wasRetrying = resolveAttemptRef.current > 0;
        resolveAttemptRef.current = 0;
        sessionIdRef.current = resolved.session_id;
        setSessionId(resolved.session_id);
        setMode(resolved.mode ?? "research");
        setBubbles(historyToBubbles(resolved.history));
        if (wasRetrying) {
          void qc.invalidateQueries({ queryKey: settingsKeys.providers() });
          void qc.invalidateQueries({ queryKey: ["chat-rail", "sessions"] });
        }
      } catch (e) {
        if (cancelled) return;
        setError(formatErr(e));
        // Self-heal: schedule a backoff retry by bumping the nonce. Without
        // this the rail would stay sessionless until a manual refresh.
        const delay =
          RESOLVE_BACKOFF_MS[
            Math.min(resolveAttemptRef.current, RESOLVE_BACKOFF_MS.length - 1)
          ]!;
        resolveAttemptRef.current += 1;
        if (resolveRetryTimerRef.current) {
          clearTimeout(resolveRetryTimerRef.current);
        }
        resolveRetryTimerRef.current = setTimeout(() => {
          setResolveNonce((n) => n + 1);
        }, delay);
      }
    })();
    return () => {
      cancelled = true;
      if (resolveRetryTimerRef.current) {
        clearTimeout(resolveRetryTimerRef.current);
        resolveRetryTimerRef.current = null;
      }
    };
  }, [abortActiveStream, open, key, scope, sessionId, variant, resolveNonce, qc]);

  useEffect(() => {
    if (variant === "desktop" && !open) abortActiveStream();
  }, [abortActiveStream, open, variant]);

  // Cancel any in-flight stream when the component unmounts.
  useEffect(
    () => () => {
      abortActiveStream();
    },
    [abortActiveStream],
  );

  const send = useCallback(
    async (text: string) => {
      if (!sessionId || !text.trim() || isStreaming) return;
      setError(null);
      const userText = text.trim();
      setInput("");
      // Anchor the new user turn to the count of assistant rows already
      // visible. Use the MAX of the bubbles-side and unified-side counts:
      // during the SSE replay window after `resolveSession`, bubbles is
      // hydrated synchronously while unifiedRows is still empty, so a
      // unified-only count would stamp anchor=0 and the merge would place
      // this user above the historical assistant. Symmetrically, multi-step
      // prior turns can produce more unified assistant rows than bubbles.
      const anchor = computeUserAnchor(bubbles, unifiedRows);
      setBubbles((b) => [
        ...b,
        { role: "user", text: userText, assistantAnchor: anchor },
        { role: "assistant", blocks: [{ kind: "text", text: "" }], tools: [] },
      ]);
      setIsStreaming(true);
      const ctrl = new AbortController();
      abortRef.current = ctrl;
      // Inner dispatcher: actually run streamChat against a given session
      // id. Pulled out so the `chat_session_missing` recovery branch can
      // re-invoke it with the freshly-resolved id without duplicating the
      // SSE plumbing.
      const dispatch = async (boundSessionId: string): Promise<void> => {
        const streamScopeKey = key;
        for await (const ev of streamChat(
          {
            session_id: boundSessionId,
            message: userText,
            provider: providerName ?? undefined,
            model: modelId.trim() || undefined,
            profile: "workspace",
          },
          ctrl.signal,
        )) {
          if (
            ctrl.signal.aborted ||
            sessionIdRef.current !== boundSessionId ||
            lastScopeKeyRef.current !== streamScopeKey
          ) {
            continue;
          }
          applyEvent(setBubbles, ev);
          invalidateForToolResult(qc, ev);
        }
      };
      try {
        await dispatch(sessionId);
      } catch (e) {
        if ((e as Error).name === "AbortError") return;
        // Self-heal: the backend reports a structurally-typed
        // `chat_session_missing` (HTTP 404 + code) when the
        // session_id the rail holds no longer exists — workspace
        // reset, factory reset, or a fresh deploy with the same
        // operator session in the browser. Resolve a fresh session
        // for the current scope and retry the message once.
        // Single-flight (recoveringSessionRef) gates concurrent
        // sends so we don't mint two replacement sessions; the
        // second caller awaits the first's resolution and reuses
        // its result.
        if (e instanceof ApiError && e.code === "chat_session_missing") {
          let recovered: string | null = null;
          try {
            const inflight =
              recoveringSessionRef.current ??
              (async () => {
                const resolved = await resolveSession(scope);
                sessionIdRef.current = resolved.session_id;
                setSessionId(resolved.session_id);
                setMode(resolved.mode ?? "research");
                resetSessionEvents(resolved.session_id);
                // Deliberately NOT calling
                // `setBubbles(historyToBubbles(resolved.history))`
                // here — a freshly-minted session has empty history,
                // and overwriting our optimistic user bubble (and
                // any earlier bubbles from the prior session that
                // the operator might still want as visual context)
                // would flicker the rail to empty before the retry
                // streams in. We keep the in-memory bubbles as-is;
                // the next non-recovery `resolveSession` (scope
                // change / startFresh) is where the history reset
                // properly belongs.
                return resolved.session_id;
              })();
            recoveringSessionRef.current = inflight;
            recovered = await inflight;
          } finally {
            recoveringSessionRef.current = null;
          }
          if (!recovered) {
            setError(formatErr(e));
          } else if (!ctrl.signal.aborted) {
            // Retry against the fresh session. The optimistic user
            // bubble we appended at send-start is still in the
            // array (we did NOT wipe it above), so no re-append is
            // needed. If this retry also fails — including a second
            // chat_session_missing — we surface the error rather
            // than looping; the operator can manually start a fresh
            // chat from the header.
            try {
              await dispatch(recovered);
            } catch (retryErr) {
              if ((retryErr as Error).name !== "AbortError") {
                setError(formatErr(retryErr));
              }
            }
          }
        } else {
          setError(formatErr(e));
        }
      } finally {
        if (abortRef.current === ctrl) {
          setIsStreaming(false);
          abortRef.current = null;
        }
      }
    },
    [
      sessionId,
      isStreaming,
      providerName,
      modelId,
      key,
      qc,
      bubbles,
      unifiedRows,
      scope,
      resetSessionEvents,
    ],
  );

  const stopStreaming = useCallback(() => {
    abortActiveStream();
  }, [abortActiveStream]);

  const startFresh = useCallback(async () => {
    abortActiveStream();
    setInput("");
    setBubbles([]);
    setError(null);
    try {
      const created = await createSession(scope);
      // Fresh session → clear any unified log carried under the new id.
      resetSessionEvents(created.session_id);
      sessionIdRef.current = created.session_id;
      setSessionId(created.session_id);
      const serverMode = created.mode ?? "research";
      const persistedMode = readPersistedMode();
      setMode(serverMode);
      setBubbles(historyToBubbles(created.history));
      lastScopeKeyRef.current = key;
      // Inherit the operator's last-used mode on the fresh session so
      // creating a new chat from Act mode doesn't silently drop back to
      // research/think.
      if (persistedMode !== serverMode) {
        try {
          const out = await setSessionMode(created.session_id, persistedMode);
          setMode(out.mode);
        } catch {
          // Best-effort: fall back to whatever the server returned.
        }
      }
      void sessionsQ.refetch();
    } catch (e) {
      setError(formatErr(e));
    }
  }, [abortActiveStream, key, scope, sessionsQ, resetSessionEvents]);

  const recentScopeSessions = useMemo(() => {
    return (sessionsQ.data ?? [])
      .filter((s) => scopeKey(s.scope) === key)
      .slice(0, 8);
  }, [key, sessionsQ.data]);

  // The thread the rail renders. Rows project from the unified session-events
  // store (`reduceRows` output) when the store has events for this session —
  // one source of truth shared with the trace dock. Until the backend mirrors
  // sends through the unified log, the legacy `bubbles` (user turns + server
  // history + live send echo) remain the baseline; the unified projection is
  // overlaid so assistant/tool/error rows from the canonical log are rendered.
  const threadBubbles = useMemo(
    () =>
      unifiedRows.length > 0
        ? mergeUnifiedRows(bubbles, unifiedRows)
        : bubbles,
    [bubbles, unifiedRows],
  );

  if (variant === "desktop" && !open) {
    return (
      <aside
        className="hidden xl:flex w-[44px] flex-col items-center gap-3 h-screen sticky top-0 border-l border-border-soft bg-surface-sidebar py-4"
        aria-label="Chat rail"
      >
        <button
          className="w-8 h-8 rounded-full flex items-center justify-center text-text-3 hover:text-text border border-border-soft"
          title="Open agent chat (⌘\\)"
          onClick={() => setOpenAndSync(true)}
        >
          <Icon name="pulse" size={14} />
        </button>
        <span className="text-[10px] font-semibold tracking-widest text-text-3 select-none"
          style={{ writingMode: "vertical-rl", transform: "rotate(180deg)" }}
        >
          CHAT
        </span>
      </aside>
    );
  }

  return (
    <aside
      className={[
        variant === "desktop"
          ? "hidden xl:flex flex-col h-screen sticky top-0 border-l border-border-soft bg-surface-sidebar"
          : "flex w-full flex-col h-full min-h-0 bg-surface-sidebar",
        className,
      ].join(" ")}
      style={variant === "desktop" ? { width: chatRailWidth + "px" } : undefined}
      aria-label="Chat rail"
    >
      {showHeader && (
        <header className="px-4 py-3 border-b border-border-soft">
          <div className="flex items-center justify-between gap-2">
            <button
              type="button"
              className="text-[12px] text-text-2 truncate flex items-center gap-1 hover:text-text"
              aria-expanded={contextMenuOpen}
              aria-controls="chat-rail-context-menu"
              onClick={() => setContextMenuOpen((v) => !v)}
              title="Switch chat context"
            >
              <span>Context ·</span>
              <span className="text-text">{CONTEXT_MODE_LABEL[contextMode]}</span>
              <Icon
                name="chevR"
                size={12}
                className={contextMenuOpen ? "rotate-90" : ""}
              />
            </button>
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
                  onClick={() => setOpenAndSync(false)}
                  title="Collapse rail"
                >
                  <Icon name="chevR" size={14} />
                </button>
              )}
            </div>
          </div>
          {contextMenuOpen && (
            <div
              id="chat-rail-context-menu"
              role="menu"
              aria-label="Chat context"
              className="mt-2 space-y-1"
            >
              {(["active", "workspace"] as RailContextMode[]).map((value) => (
                <button
                  key={value}
                  type="button"
                  role="menuitemradio"
                  aria-checked={contextMode === value}
                  onClick={() => selectContextMode(value)}
                  className={[
                    "w-full text-left text-[11px] px-2 py-1 rounded-sm border",
                    contextMode === value
                      ? "border-border bg-surface-elev text-text"
                      : "border-border-soft text-text-3 hover:text-text",
                  ].join(" ")}
                >
                  {CONTEXT_MODE_LABEL[value]}
                </button>
              ))}
            </div>
          )}
        </header>
      )}
      {showHeader && recentScopeSessions.length > 0 && (
        <div className="px-4 py-2 border-b border-border-soft bg-surface-2/20">
          <button
            type="button"
            className="mb-1 flex w-full items-center justify-between text-left text-[11px] text-text-3 hover:text-text"
            aria-expanded={!historyCollapsed}
            onClick={() => {
              const next = !historyCollapsed;
              setHistoryCollapsed(next);
              safeStorageSet(RAIL_HISTORY_COLLAPSED_LS, next ? "1" : "0");
            }}
          >
            <span>Conversation history</span>
            <Icon
              name="chevR"
              size={12}
              className={historyCollapsed ? "" : "rotate-90"}
            />
          </button>
          {!historyCollapsed && (
            <div className="space-y-1">
              {recentScopeSessions.map((s) => {
                const isActive = s.id === sessionId;
                // First-turn snippets only available for the active
                // session (we have its bubbles); for other rows the
                // hook falls back to cache/localStorage or the date.
                const activeFirstUser = isActive ? firstUserText(bubbles) : undefined;
                const activeFirstAssistant = isActive
                  ? firstAssistantText(bubbles)
                  : undefined;
                return (
                  <ChatHistoryItem
                    key={s.id}
                    sessionId={s.id}
                    lastActivityAt={s.last_activity_at}
                    isActive={isActive}
                    firstUser={activeFirstUser}
                    firstAssistant={activeFirstAssistant}
                    providerName={providerName}
                    modelId={modelId}
                    providersConfigured={
                      (providers.data?.providers ?? []).length > 0
                    }
                    ready={isActive && !isStreaming && !!activeFirstAssistant}
                    onClick={async () => {
                      abortActiveStream();
                      try {
                        sessionIdRef.current = s.id;
                        setSessionId(s.id);
                        setMode(s.mode ?? "research");
                        const h = await loadSessionHistory(s.id);
                        setBubbles(historyToBubbles(h));
                      } catch (e) {
                        setError(formatErr(e));
                      }
                    }}
                  />
                );
              })}
            </div>
          )}
        </div>
      )}

      <RailModelBar
        rows={providers.data?.providers ?? []}
        loading={providers.isPending}
        provider={providerName}
        model={modelId}
        modelSourceLabel={railModelSourceLabel}
        mode={mode}
        modePending={modePending}
        modeDisabled={!sessionId || isStreaming}
        onModeChange={async (next) => {
          if (!sessionId || modePending || next === mode) return;
          setModePending(true);
          setError(null);
          try {
            const out = await setSessionMode(sessionId, next);
            setMode(out.mode);
            // When switching to Act, prefer whatever the operator already
            // typed into the composer — sending the hardcoded
            // "Continue in Act mode." over the top of pending text
            // overrides the user's intent and surfaced as a recurring QA
            // complaint. The continuation prompt only fires as a fallback
            // when the composer is empty AND there is a blocked tool call
            // waiting on Act-mode authorization.
            if (out.mode === "act") {
              const pending = input.trim();
              if (pending) {
                void send(pending);
              } else if (hasBlockedToolCall(threadBubbles)) {
                void send("Continue in Act mode.");
              }
            }
          } catch (e) {
            setError(formatErr(e));
          } finally {
            setModePending(false);
          }
        }}
        onChange={(p, m) => {
          setProviderName(p);
          setModelId(m);
          if (p) safeStorageSet(RAIL_PROVIDER_LS, p);
          else safeStorageRemove(RAIL_PROVIDER_LS);
          if (m) safeStorageSet(RAIL_MODEL_LS, m);
          else safeStorageRemove(RAIL_MODEL_LS);
        }}
      />

      <ChatThread bubbles={threadBubbles} isStreaming={isStreaming} />

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

      {/*
        `flex-shrink-0` so the composer never gets squeezed off-screen
        when the thread runs long on shorter viewports — the rail
        column is `h-screen` / `h-full min-h-0` and `ChatThread` is
        `flex-1`, so a non-shrinking composer is the only way to
        guarantee the input lives at the bottom of the dialog. QA
        flagged "the chat … do not appear at bottom of dialog in chat
        rail" — the composer's wrapper had no shrink guard.
      */}
      <div className="flex-shrink-0">
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
      </div>
    </aside>
  );
}

function RailModelBar({
  rows,
  loading,
  provider,
  model,
  modelSourceLabel,
  mode,
  modePending,
  modeDisabled,
  onModeChange,
  onChange,
}: {
  rows: ProviderRow[];
  loading: boolean;
  provider: string | null;
  model: string;
  modelSourceLabel: string;
  mode: ChatSessionMode;
  modePending: boolean;
  modeDisabled: boolean;
  onModeChange: (mode: ChatSessionMode) => void | Promise<void>;
  onChange: (provider: string | null, model: string) => void;
}) {
  return (
    <div className="border-b border-border-soft px-4 py-2 bg-surface-2/30 space-y-2">
      <div className="flex items-center gap-2">
        <label className="text-[11px] text-text-3 uppercase tracking-wider">
          Model
        </label>
        <ModelPicker
          rows={rows}
          loading={loading}
          provider={provider}
          model={model}
          onChange={onChange}
          className="flex-1 min-w-0"
          ariaLabel="Model"
          emptyHint="no models picked — visit Settings → Providers"
        />
      </div>
      <p className="m-0 text-[11px] text-text-3">
        {modelSourceLabel}
      </p>
      <div
        role="group"
        aria-label="Chat mode"
        className="grid grid-cols-2 overflow-hidden rounded border border-border-soft"
      >
        {(["research", "act"] as ChatSessionMode[]).map((value) => (
          <button
            key={value}
            type="button"
            disabled={modeDisabled || modePending}
            onClick={() => void onModeChange(value)}
            aria-pressed={mode === value}
            className={[
              "h-7 text-[11px] font-semibold uppercase tracking-wider transition-colors disabled:cursor-not-allowed disabled:opacity-60",
              mode === value
                ? value === "act"
                  ? "bg-gold text-bg"
                  : "bg-surface-elev text-text"
                : "bg-transparent text-text-3 hover:text-text",
            ].join(" ")}
          >
            {value === "act" ? "ACT" : "THINK"}
          </button>
        ))}
      </div>
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
/**
 * Anchor a new user turn to the count of assistant rows already visible.
 * Returns the max of the assistant counts in `bubbles` and `unifiedRows`
 * so the merge places the user *after* whichever side currently leads —
 * critical during the SSE replay window when bubbles is hydrated but
 * unifiedRows hasn't caught up.
 */
export function computeUserAnchor(
  bubbles: Bubble[],
  unifiedRows: MessageRow[],
): number {
  let inBubbles = 0;
  for (const b of bubbles) if (b.role === "assistant") inBubbles += 1;
  let inUnified = 0;
  for (const r of unifiedRows) if (r.type === "assistant") inUnified += 1;
  return inBubbles > inUnified ? inBubbles : inUnified;
}

/** First user-turn text in a bubble list, or undefined if none yet. */
function firstUserText(bubbles: Bubble[]): string | undefined {
  for (const b of bubbles) if (b.role === "user") return b.text;
  return undefined;
}

/** First assistant-turn text in a bubble list, or undefined if none yet. */
function firstAssistantText(bubbles: Bubble[]): string | undefined {
  for (const b of bubbles) {
    if (b.role === "assistant") {
      const parts = b.blocks
        .map((blk) => (blk.kind === "text" ? blk.text : ""))
        .filter(Boolean);
      const joined = parts.join(" ").trim();
      if (joined) return joined;
    }
  }
  return undefined;
}

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

/**
 * Project the unified `MessageRow[]` (the canonical reducer output shared
 * with the trace dock) onto the rail's bubble model, then merge with the
 * legacy `bubbles` baseline.
 *
 * The unified log is authoritative for assistant / tool / checkpoint / error
 * rows; legacy `bubbles` is authoritative only for USER turns (the rail's
 * POST send path doesn't go through the unified projector yet). Each user
 * bubble carries `assistantAnchor` — the number of assistant rows that had
 * already closed when the user submitted — so we can interleave the user
 * turn at its true chronological position even when a single legacy
 * assistant slot expands into multiple unified assistant rows (multi-step
 * agent turns).
 *
 * The trailing optimistic assistant bubble (the empty placeholder pushed on
 * send so typing dots have a home before the first token arrives) is
 * appended last, but only when the projection has not yet produced an
 * assistant row to cover it.
 */
export function mergeUnifiedRows(bubbles: Bubble[], rows: MessageRow[]): Bubble[] {
  const projected = unifiedRowsToBubbles(rows);
  if (projected.length === 0) return bubbles;

  // Checkpoint rollback hides any user-turn bubble whose
  // `assistantAnchor` falls inside a rolled-back range (the
  // chat-rollback fix landed on the remote branch in parallel —
  // commit b4f98653). Kept verbatim alongside the new merge-anchor
  // changes.
  const rolledBackAnchorRanges = checkpointRollbackAnchorRanges(rows);
  const isRolledBackUser = (anchor: number) =>
    rolledBackAnchorRanges.some(({ from, to }) => anchor >= from && anchor < to);

  const projectedAssistantCount = projected.filter(
    (p) => p.role === "assistant",
  ).length;

  // `assistantAnchor` is the count of CLOSED assistant rows at the
  // moment the user pressed send — captured in `send()` so we can
  // re-insert the user bubble at its true chronological position even
  // when one legacy assistant slot fans out into multiple unified
  // assistant rows (multi-step agent turns).
  //
  // The fallback is for OLD bubbles persisted before `assistantAnchor`
  // existed in the snapshot, or for hand-built test fixtures. The
  // previous fallback (`users.length` — the user-count) was in the
  // wrong UNIT: downstream we compare against `projAssistantCount`, so
  // a fallback in user-count would interleave such a bubble into the
  // middle of the projection and produce the user-message-over-the-
  // top-of-the-agent-bubble overlap the QA report flagged. Anchoring
  // to `projectedAssistantCount` instead sorts unanchored users to
  // the end (after every projected assistant), preserving insertion
  // order via the stable sort below.
  type AnchoredUser = { user: Bubble; anchor: number };
  const users: AnchoredUser[] = [];
  for (const b of bubbles) {
    if (b.role === "user") {
      const anchor = b.assistantAnchor ?? projectedAssistantCount;
      if (!isRolledBackUser(anchor)) {
        users.push({ user: b, anchor });
      }
    }
  }
  users.sort((a, b) => a.anchor - b.anchor);

  // The optimistic trailing assistant placeholder is the empty bubble pushed
  // by `send()` so typing dots have a home before the first token arrives.
  // Keep it ONLY while the projection has not yet produced an assistant row
  // for the most-recently-sent user turn — once the row exists, projection
  // owns the rendering and the placeholder would just duplicate it.
  const last = bubbles[bubbles.length - 1];
  const lastUserAnchor =
    users.length > 0 ? users[users.length - 1].anchor : 0;
  const trailingOptimistic =
    last &&
    last.role === "assistant" &&
    projectedAssistantCount <= lastUserAnchor
      ? last
      : null;

  const out: Bubble[] = [];
  let projAssistantCount = 0;
  let userIdx = 0;
  while (userIdx < users.length && users[userIdx].anchor <= 0) {
    out.push(users[userIdx].user);
    userIdx += 1;
  }
  for (const p of projected) {
    if (p.role === "assistant") {
      projAssistantCount += 1;
    }
    out.push(p);
    while (
      userIdx < users.length &&
      users[userIdx].anchor <= projAssistantCount
    ) {
      out.push(users[userIdx].user);
      userIdx += 1;
    }
  }
  while (userIdx < users.length) {
    out.push(users[userIdx].user);
    userIdx += 1;
  }
  if (trailingOptimistic) out.push(trailingOptimistic);
  return out;
}

/** One assistant bubble per assistant row; tool/error/etc. rows attach to or
 *  follow the nearest preceding assistant bubble (or open their own).
 *  Checkpoint rows emit a standalone checkpoint bubble so they render as a
 *  clickable rewind affordance, ordered inline by `seq`.
 *
 *  Rollback semantics: when a `checkpoint_restored` row exists for some
 *  checkpoint id, every non-checkpoint row whose `seq` falls strictly between
 *  the original `checkpoint_created` row's seq and the restored row's seq is
 *  hidden — those messages were rolled back on the server. Checkpoint rows
 *  themselves are preserved so the operator can still see the rewind marker. */
function unifiedRowsToBubbles(rows: MessageRow[]): Bubble[] {
  const rolledBackRanges = checkpointRollbackSeqRanges(rows);
  const isRolledBack = (seq: number) =>
    rolledBackRanges.some(({ from, to }) => seq > from && seq < to);

  const out: Bubble[] = [];
  let current: AssistantBubble | null = null;

  // QA30: when a non-assistant row (tool / error / optimizer) arrives
  // and `current` is null — e.g. immediately after a checkpoint reset,
  // or before the first assistant row of a turn has been projected —
  // prefer attaching to the LAST assistant bubble already in `out`
  // rather than minting a fresh empty bubble for it. The previous
  // behaviour would orphan tool rows into a standalone empty assistant
  // bubble that rendered above (or below) the real response, surfacing
  // as "tool call stayed open / appears twice / appears above the
  // agent message" in QA reports.
  const ensureBubble = (): AssistantBubble => {
    if (!current) {
      for (let i = out.length - 1; i >= 0; i -= 1) {
        const cand = out[i];
        if (cand.role === "assistant") {
          current = cand;
          return current;
        }
        if (cand.role === "checkpoint") break;
      }
      current = { role: "assistant", blocks: [], tools: [] };
      out.push(current);
    }
    return current;
  };

  for (const row of rows) {
    // Suppress rolled-back content; keep checkpoint markers so the rewind is
    // visible inline. (Each checkpoint row is the boundary of its own range —
    // never strictly inside one — so `isRolledBack` never matches them.)
    if (row.type !== "checkpoint" && isRolledBack(row.seq)) continue;
    switch (row.type) {
      case "assistant": {
        // Each assistant row is its own bubble (messageIndex-distinct).
        current = { role: "assistant", blocks: [], tools: [] };
        if (row.text) current.blocks.push({ kind: "text", text: row.text });
        for (const block of row.blocks) {
          current.blocks.push(
            contentBlockToRenderable(block as ContentBlock),
          );
        }
        out.push(current);
        break;
      }
      case "tool": {
        ensureBubble().tools.push(toolRowToTool(row));
        break;
      }
      case "error": {
        const b = ensureBubble();
        appendAssistantText(
          b,
          `\n\n[${row.errorKind} · ${row.code}] ${row.message}`,
        );
        break;
      }
      case "checkpoint": {
        // The render layer (`ChatBubble`) currently SUPPRESSES the
        // checkpoint bubble per QA — see SHOW_CHECKPOINTS_IN_RAIL in
        // ChatBubble.tsx — but the projection still emits the row
        // so the chat-rollback logic (`isRolledBackUser`, used by
        // `mergeUnifiedRows` above) can compute its "user turns
        // inside a rewound window" range correctly.
        out.push({
          role: "checkpoint",
          checkpointId: row.checkpointId,
          status: row.status,
          message: row.message,
        });
        // A subsequent text row should start a fresh assistant bubble after
        // the checkpoint so we don't fold it into the prior one.
        current = null;
        break;
      }
      case "optimizer": {
        const b = ensureBubble();
        const tail = row.completed
          ? row.mintedAgentId
            ? ` → minted ${row.mintedAgentId}`
            : " → completed"
          : ` · ${row.candidateCount} candidate(s)`;
        appendAssistantText(b, `\n\n[optimizer ${row.optimizationId}${tail}]`);
        break;
      }
    }
  }
  return out;
}

function checkpointRollbackSeqRanges(rows: MessageRow[]): Array<{ from: number; to: number }> {
  // Locate rolled-back event ranges. We compose multiple restores naturally
  // because each restored event contributes its own (from, to) interval.
  const createdSeqByCheckpoint = new Map<string, number>();
  const rolledBackRanges: Array<{ from: number; to: number }> = [];
  for (const row of [...rows].sort((a, b) => a.seq - b.seq)) {
    if (row.type !== "checkpoint") continue;
    if (row.status === "created") {
      createdSeqByCheckpoint.set(row.checkpointId, row.seq);
    } else if (row.status === "restored") {
      const createdSeq = createdSeqByCheckpoint.get(row.checkpointId);
      if (createdSeq != null && row.seq > createdSeq) {
        rolledBackRanges.push({ from: createdSeq, to: row.seq });
      }
    }
  }
  return rolledBackRanges;
}

function checkpointRollbackAnchorRanges(rows: MessageRow[]): Array<{ from: number; to: number }> {
  // Legacy user bubbles do not carry unified `seq`; they only know how many
  // assistant rows existed when the user turn was sent. Convert checkpoint
  // rollback seq windows into the same assistant-count coordinate system so
  // user turns sent after the checkpoint are hidden with the rolled-back
  // assistant/tool rows.
  const createdAnchorByCheckpoint = new Map<string, number>();
  const ranges: Array<{ from: number; to: number }> = [];
  let assistantCount = 0;
  for (const row of [...rows].sort((a, b) => a.seq - b.seq)) {
    if (row.type === "assistant") {
      assistantCount += 1;
      continue;
    }
    if (row.type !== "checkpoint") continue;
    if (row.status === "created") {
      createdAnchorByCheckpoint.set(row.checkpointId, assistantCount);
    } else if (row.status === "restored") {
      const createdAnchor = createdAnchorByCheckpoint.get(row.checkpointId);
      if (createdAnchor != null && assistantCount > createdAnchor) {
        ranges.push({ from: createdAnchor, to: assistantCount });
      }
    }
  }
  return ranges;
}

function toolRowToTool(row: ToolRow): Tool {
  const terminal =
    row.status === "finished" ||
    row.status === "failed" ||
    row.status === "cancelled" ||
    row.status === "denied";
  const blocked =
    row.status === "denied" ||
    row.policyOutcome === "denied" ||
    row.policyOutcome === "needs_approval";
  const ok = row.status !== "failed" && !blocked;
  const blockedSummary =
    row.policyOutcome === "needs_approval"
      ? "needs approval"
      : row.policyOutcome === "denied"
        ? "denied"
        : null;
  const summaryBits = [row.policyOutcome, row.outputHash ? "ok" : null].filter(
    Boolean,
  ) as string[];
  const errorMessage =
    row.errorMessage ??
    (blockedSummary ? `Tool ${blockedSummary}.` : null);
  return {
    call: row.toolName ?? row.spanId,
    ok,
    summary: summaryBits.join(" · "),
    resultSummary: errorMessage ?? (row.outputHash ? "ok" : ""),
    pending: !terminal,
    result: errorMessage ? { error: errorMessage } : undefined,
  };
}

function hasBlockedToolCall(bubbles: Bubble[]): boolean {
  for (let i = bubbles.length - 1; i >= Math.max(0, bubbles.length - 6); i--) {
    const b = bubbles[i];
    if (!b || b.role !== "assistant") continue;
    if (
      b.tools.some((t) => {
        if (t.ok) return false;
        const detail = [
          t.summary,
          t.resultSummary,
          typeof (t.result as { error?: unknown } | undefined)?.error === "string"
            ? String((t.result as { error?: string }).error)
            : "",
        ]
          .join(" ")
          .toLowerCase();
        return (
          detail.includes("research mode") ||
          detail.includes("needs approval") ||
          detail.includes("denied")
        );
      })
    ) {
      return true;
    }
  }
  return false;
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
      a.tools = a.tools.map((t) =>
        t.pending
          ? {
              ...t,
              ok: false,
              pending: false,
              resultSummary: ev.message,
              result: { error: ev.message },
            }
          : t,
      );
    } else if (ev.type === "done") {
      a.tools = a.tools.map((t) =>
        t.pending ? { ...t, pending: false } : t,
      );
    }
    next[next.length - 1] = a;
    return next;
  });
}

function historyToBubbles(history: ChatMessage[]): Bubble[] {
  const out: Bubble[] = [];
  let pendingAssistant: AssistantBubble | null = null;
  // Number of assistant chat-messages already emitted — written onto every
  // user bubble as `assistantAnchor` so the unified-merge can interleave the
  // user turn at the chronologically correct spot (matters for multi-step
  // turns where a single legacy assistant message may correspond to several
  // unified assistant rows).
  let assistantCount = 0;

  // First pass: collect assistant text + tool_use blocks per message.
  // Then attach matching tool_results from subsequent user messages onto
  // the prior assistant bubble's tool list.
  for (const cm of history) {
    if (cm.role === "user") {
      if (pendingAssistant) {
        out.push(pendingAssistant);
        pendingAssistant = null;
        assistantCount += 1;
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
                "error" in parsedResult &&
                Boolean((parsedResult as { error?: unknown }).error);
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
        if (text)
          out.push({ role: "user", text, assistantAnchor: assistantCount });
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
