import { useEffect, useState } from "react";
import { createSession, deleteSession, streamChat } from "@/api/chat_rail";

// Module-level caches so the hook is single-fire per session id even
// across remounts (StrictMode + route navigation). localStorage gives
// best-effort cross-reload persistence; the chat-rail does not yet
// have a server-side title field (see contract acceptance — backend
// persistence is a queue follow-up).
const TITLE_STORAGE_PREFIX = "xvn.chat-history.title.";
const titleCache = new Map<string, string>();
const inflight = new Set<string>();
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
  const fresh = await createSession({ scope: "workspace" });
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
  const [title, setTitle] = useState<string | null>(
    () => titleCache.get(sessionId) ?? readPersisted(sessionId),
  );

  useEffect(() => {
    if (title) return;
    if (!ready) return;
    if (!firstUser || !firstAssistant) return;
    if (!providersConfigured || !providerName) {
      console.warn(
        "[chat-title] no provider configured; falling back to date label",
      );
      return;
    }
    if (inflight.has(sessionId) || failed.has(sessionId)) return;

    inflight.add(sessionId);
    let cancelled = false;
    const ctrl = new AbortController();

    (async () => {
      try {
        const t = await summarizeChatTitle({
          firstUser,
          firstAssistant,
          providerName,
          modelId,
          signal: ctrl.signal,
        });
        if (cancelled) return;
        titleCache.set(sessionId, t);
        writePersisted(sessionId, t);
        setTitle(t);
      } catch (e) {
        if (cancelled) return;
        failed.add(sessionId);
        console.warn(
          "[chat-title] summarize failed; falling back to date label",
          e,
        );
      } finally {
        inflight.delete(sessionId);
      }
    })();

    return () => {
      cancelled = true;
      ctrl.abort();
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
  inflight.clear();
  failed.clear();
}
