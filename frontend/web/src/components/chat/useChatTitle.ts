import { useEffect, useState } from "react";
import { createSession, deleteSession, streamChat } from "@/api/chat_rail";

// Module-level caches so the hook is single-fire per session id even
// across remounts (StrictMode + route navigation). localStorage gives
// best-effort cross-reload persistence; the chat-rail does not yet
// have a server-side title field (see contract acceptance — backend
// persistence is a queue follow-up).
const TITLE_STORAGE_PREFIX = "xvn.chat-history.title.";
const TITLE_HELPER_SCOPE_TAG = "__xvn-chat-title-helper";
const titleCache = new Map<string, string>();
// Shared in-flight promises so React StrictMode double-invoke does not
// abort the only run and leave the second effect short-circuited by an
// `inflight` guard with no retry pending. Each effect observer just
// subscribes to the shared promise and skips its own setState on
// cleanup (PR #280 review).
const inflightPromises = new Map<string, Promise<string>>();
const failed = new Set<string>();

function storageKey(sessionId: string): string {
  return `${TITLE_STORAGE_PREFIX}${sessionId}`;
}

function readPersisted(sessionId: string): string | null {
  try {
    return localStorage.getItem(storageKey(sessionId));
  } catch {
    return null;
  }
}

function writePersisted(sessionId: string, title: string): void {
  try {
    localStorage.setItem(storageKey(sessionId), title);
  } catch {
    // best effort
  }
}

/** Trim, strip wrapping quotes / terminal punctuation, clamp to 7 words. */
export function cleanTitle(raw: string): string {
  let t = raw.trim();
  t = t.replace(/^["'`“”]+|["'`“”]+$/g, "");
  t = t.replace(/[.!?]+$/g, "");
  const words = t.split(/\s+/).filter(Boolean).slice(0, 7);
  return words.join(" ").slice(0, 80);
}

function summarizePrompt(firstUser: string, firstAssistant: string): string {
  return (
    `Summarize this conversation in <=7 words, no quotes, no terminal punctuation. ` +
    `Return only the title text, nothing else.\n\n` +
    `User: ${firstUser.slice(0, 400)}\n` +
    `Assistant: ${firstAssistant.slice(0, 400)}`
  );
}

/**
 * Dispatch a one-shot summarize against the configured provider/model
 * and return a cleaned title. Creates a throwaway chat session so the
 * summarize turn doesn't pollute the operator's real thread, then
 * best-effort deletes it.
 */
export async function summarizeChatTitle({
  firstUser,
  firstAssistant,
  providerName,
  modelId,
  signal,
}: {
  firstUser: string;
  firstAssistant: string;
  providerName: string;
  modelId?: string;
  signal?: AbortSignal;
}): Promise<string> {
  // Use a unique `selection` scope tagged `__xvn-chat-title-helper` so
  // the throwaway session can NEVER be returned by `resolve(operator-
  // scope)` (`resolve` matches by exact context_scope_json) and can
  // never land in the operator-visible history pane (the rail filters
  // sessions by scopeKey match, which is unique-per-call here). The
  // best-effort delete in `finally` still cleans up the row after the
  // model call returns. PR #280 review.
  const helperToken =
    typeof crypto !== "undefined" && "randomUUID" in crypto
      ? crypto.randomUUID()
      : `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  const fresh = await createSession({
    scope: "selection",
    items: [TITLE_HELPER_SCOPE_TAG, helperToken],
  });
  try {
    let body = "";
    for await (const ev of streamChat(
      {
        session_id: fresh.session_id,
        message: summarizePrompt(firstUser, firstAssistant),
        provider: providerName,
        model: modelId,
        profile: "workspace",
      },
      signal,
    )) {
      if (ev.type === "token") {
        body += ev.text;
      } else if (ev.type === "error") {
        throw new Error(ev.message);
      } else if (ev.type === "done") {
        break;
      }
    }
    const title = cleanTitle(body);
    if (!title) throw new Error("summarizer returned empty title");
    return title;
  } finally {
    // Cleanup the throwaway session. Swallow any error so a failed
    // delete doesn't taint the returned title.
    void deleteSession(fresh.session_id).catch(() => {});
  }
}

/**
 * Resolve the auto-title for a chat session. Returns `null` to mean
 * "no title — caller falls back to the date label". The summarize
 * dispatcher fires once per session id (module-level guard), only
 * when the thread has both a first user turn and a first assistant
 * response and the workspace has a provider configured.
 */
export function useChatTitle({
  sessionId,
  firstUser,
  firstAssistant,
  providerName,
  modelId,
  providersConfigured,
  ready,
}: {
  sessionId: string;
  firstUser: string | undefined;
  firstAssistant: string | undefined;
  providerName: string | null;
  modelId: string;
  providersConfigured: boolean;
  /** Gate: only fire once the first assistant response is complete. */
  ready: boolean;
}): string | null {
  const [titleEntry, setTitleEntry] = useState<{
    id: string;
    value: string;
  } | null>(() => {
    const t = titleCache.get(sessionId) ?? readPersisted(sessionId);
    return t ? { id: sessionId, value: t } : null;
  });
  // Null when sessionId changed since last render — prevents stale title leaking.
  const title = titleEntry?.id === sessionId ? titleEntry.value : null;

  useEffect(() => {
    if (!title) {
      const cached = titleCache.get(sessionId) ?? readPersisted(sessionId);
      if (cached) {
        setTitleEntry({ id: sessionId, value: cached });
        return;
      }
    }
    if (title) return;
    if (!ready) return;
    if (!firstUser || !firstAssistant) return;
    if (!providersConfigured || !providerName) {
      console.warn(
        "[chat-title] no provider configured; falling back to date label",
      );
      return;
    }
    if (failed.has(sessionId)) return;

    let cancelled = false;
    // Share a single in-flight promise across remounts so React
    // StrictMode's deliberate double-invoke can't abort the only
    // request and leave the second effect short-circuited by an
    // `inflight` guard with no retry. The promise outlives every
    // effect instance; effects only subscribe + skip setState on
    // cleanup.
    let promise = inflightPromises.get(sessionId);
    if (!promise) {
      promise = summarizeChatTitle({
        firstUser,
        firstAssistant,
        providerName,
        modelId,
      })
        .then((t) => {
          titleCache.set(sessionId, t);
          writePersisted(sessionId, t);
          return t;
        })
        .catch((e) => {
          failed.add(sessionId);
          console.warn(
            "[chat-title] summarize failed; falling back to date label",
            e,
          );
          // Re-throw so subscribers also fall through their catch.
          throw e;
        })
        .finally(() => {
          inflightPromises.delete(sessionId);
        });
      inflightPromises.set(sessionId, promise);
    }

    promise
      .then((t) => {
        if (cancelled) return;
        setTitleEntry({ id: sessionId, value: t });
      })
      .catch(() => {
        // Already logged inside the shared promise.
      });

    return () => {
      cancelled = true;
    };
  }, [
    sessionId,
    ready,
    providersConfigured,
    providerName,
    modelId,
    firstUser,
    firstAssistant,
    title,
  ]);

  return title;
}

/** Test-only: reset module-level guards between tests. */
export function __resetChatTitleForTests() {
  titleCache.clear();
  inflightPromises.clear();
  failed.clear();
}
