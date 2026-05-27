import { useCallback, useState } from "react";

import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

import { ContentBlockView } from "@/components/chat/ContentBlockView";

import type { Bubble, CheckpointBubble, RenderableBlock, Tool } from "./types";

/**
 * Temporary flag — when `false`, the rail SKIPS RENDERING checkpoint
 * bubbles. The projection layer (`unifiedRowsToBubbles` in
 * `ChatRail.tsx`) still emits the rows so chat-rollback can compute
 * which user turns fall inside a rewound window; only the visual
 * surface is muted.
 *
 * Why this is here, not at the merge layer: a `mergeUnifiedRows`-
 * level suppression would also hide the rows from the rollback
 * computation and from `mergeUnifiedRows`-based test fixtures. The
 * QA complaint is purely visual ("Checkpoints appear suddenly all
 * at once at end of 4th turn") — fix it at the render boundary.
 *
 * Re-enable once the server-side emission is interleaved with the
 * turn (currently flushes as a batch at turn close, which produces
 * the "all at once" complaint).
 */
const SHOW_CHECKPOINTS_IN_RAIL = false;

export function ChatBubble({
  bubble,
  isLast,
  isStreaming,
}: {
  bubble: Bubble;
  isLast: boolean;
  isStreaming: boolean;
}) {
  if (bubble.role === "user") {
    return (
      <div className="min-w-0 max-w-[92%] self-end">
        <div className="break-words bg-blue-500/10 dark:bg-blue-400/10 border border-blue-500/30 dark:border-blue-400/30 rounded-md px-2.5 py-1.5 text-[13px] whitespace-pre-wrap leading-snug">
          {bubble.text}
        </div>
      </div>
    );
  }

  if (bubble.role === "checkpoint") {
    // QA "Checkpoints appear suddenly all at once at end of 4th
    // turn" — the underlying server-side emission currently queues
    // checkpoints and flushes them as a batch at turn close, so
    // they all materialize together rather than inline at the
    // point of the rewind. Hide them in the rail until that
    // batching is fixed server-side. The merge layer keeps emitting
    // checkpoint rows so the chat-rollback logic can still compute
    // which user turns fall inside a rewound window — it just no
    // longer surfaces a bubble in the rail.
    if (!SHOW_CHECKPOINTS_IN_RAIL) return null;
    return <CheckpointRow bubble={bubble} />;
  }

  const showDots = isStreaming && isLast;
  const hasRenderableBlocks = bubble.blocks.some(
    (block) => block.kind !== "text" || block.text.length > 0,
  );
  const hasContent = hasRenderableBlocks || bubble.tools.length > 0;

  return (
    <div className="min-w-0 max-w-[92%] self-start">
      {hasRenderableBlocks ? (
        <div className="break-words bg-surface-2/60 border border-border rounded-md px-2.5 py-1.5 text-[13px] leading-snug">
          <ContentBlocksView blocks={bubble.blocks} />
          {showDots && <TypingDots inline />}
        </div>
      ) : showDots ? (
        <div className="break-words bg-surface-2/60 border border-border rounded-md px-2.5 py-1.5 text-[13px] leading-snug">
          <TypingDots />
        </div>
      ) : hasContent ? null : null}
      {bubble.tools.length > 0 && (
        <div className={`${hasRenderableBlocks ? "mt-1.5" : ""} flex flex-col gap-1`}>
          {bubble.tools.map((t, i) => (
            <ToolButton key={i} tool={t} />
          ))}
        </div>
      )}
    </div>
  );
}

function ToolButton({ tool }: { tool: Tool }) {
  const ok = tool.ok;
  const pending = tool.pending;
  const label = friendlyToolLabel(tool);
  const status = friendlyToolStatus(tool);
  const failed = !ok && !pending;
  // QA flagged the tool cards as having a "hard white border" — the
  // border tone was set at /40 opacity against the rail surface,
  // which read as a bright outline. Softened to /15 (and the
  // neutral pending state to fully transparent) so the cards sit on
  // the rail without competing with the surrounding text. The shape
  // / colour intent is unchanged; only the contrast on the outline
  // is dialled back.
  const tone = !ok
    ? "border-border-soft bg-danger/[0.08] text-danger"
    : pending
      ? "border-border-soft/70 bg-surface-2/50 text-text-2"
      : "border-border-soft bg-success/[0.08] text-success";
  return (
    <div
      className={`inline-flex items-center gap-1.5 self-start max-w-full px-2 py-1 rounded-md border text-[12px] ${tone}`}
    >
      {pending ? (
        <span
          className="inline-block w-2.5 h-2.5 border border-current border-t-transparent rounded-full animate-spin flex-shrink-0"
          aria-label="running"
        />
      ) : ok ? (
        <span aria-hidden className="flex-shrink-0">✓</span>
      ) : (
        <span aria-hidden className="flex-shrink-0">!</span>
      )}
      <span className="font-mono truncate">
        {label}
        {failed ? " failed:" : ""}
      </span>
      {status && (
        <span className="text-text-3 truncate" title={status}>
          {failed ? status : `· ${status}`}
        </span>
      )}
    </div>
  );
}

function friendlyToolLabel(t: Tool): string {
  return t.call;
}

function friendlyToolStatus(t: Tool): string {
  const result = (t.result ?? {}) as Record<string, unknown>;
  if (typeof result.error === "string") return result.error;
  if (t.pending) return t.summary ?? "running…";
  return t.resultSummary ?? t.summary ?? "ok";
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
          return <ContentBlockView key={index} block={block.block} />;
        }
        return <UnsupportedBlockNotice key={index} type={block.type} />;
      })}
    </div>
  );
}

function UnsupportedBlockNotice({ type }: { type: string }) {
  return (
    <div className="rounded border border-border-soft bg-surface-elev px-2 py-1 text-[12px] text-text-3">
      Unsupported chat block: <span className="font-mono">{type}</span>
    </div>
  );
}

function MarkdownView({ text }: { text: string }) {
  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm]}
      components={{
        // Inline code keeps a soft background; block code is wrapped in <pre>
        // which carries its own background, so suppress the inline styling
        // when ReactMarkdown gives the <code> a language- className (block).
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        code: ({ children, className, ...props }: any) => (
          <code
            className={`font-mono text-[12px] ${
              className ? "" : "bg-surface-2/70 px-1 py-0.5 rounded"
            }`}
            {...props}
          >
            {children}
          </code>
        ),
        // The rail is a ~380px panel; horizontal scroll inside a
        // sidebar reads as broken. Wrap long lines instead. QA also
        // flagged single-line code overflowing without wrapping —
        // `whitespace-pre-wrap` keeps intentional newlines but allows
        // soft-breaks within long unbroken tokens (URLs, JSON one-
        // liners). `break-words` is the last-resort for tokens with
        // no break opportunities at all.
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        pre: ({ children }: any) => (
          <pre className="font-mono text-[12px] bg-surface-2/70 p-2 rounded my-1.5 whitespace-pre-wrap break-words">
            {children}
          </pre>
        ),
        table: ({ children }) => (
          <div className="overflow-x-auto my-1.5">
            <table className="border-collapse text-[12px]">{children}</table>
          </div>
        ),
        th: ({ children }) => (
          <th className="border border-border-soft px-1.5 py-1 text-left font-medium">
            {children}
          </th>
        ),
        td: ({ children }) => (
          <td className="border border-border-soft px-1.5 py-1">{children}</td>
        ),
        ul: ({ children }) => (
          <ul className="list-disc pl-4 my-1 space-y-0.5">{children}</ul>
        ),
        ol: ({ children }) => (
          <ol className="list-decimal pl-4 my-1 space-y-0.5">{children}</ol>
        ),
        p: ({ children }) => (
          <p className="my-1 first:mt-0 last:mb-0">{children}</p>
        ),
        strong: ({ children }) => (
          <strong className="text-text font-semibold">{children}</strong>
        ),
        h1: ({ children }) => (
          <h1 className="text-[14px] font-semibold my-1.5">{children}</h1>
        ),
        h2: ({ children }) => (
          <h2 className="text-[14px] font-semibold my-1.5">{children}</h2>
        ),
        h3: ({ children }) => (
          <h3 className="text-[13px] font-semibold my-1">{children}</h3>
        ),
        a: ({ children, href }) => (
          <a
            href={href}
            className="text-gold underline decoration-gold/40 hover:decoration-gold"
            target="_blank"
            rel="noopener noreferrer"
          >
            {children}
          </a>
        ),
      }}
    >
      {text}
    </ReactMarkdown>
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

function CheckpointRow({ bubble }: { bubble: CheckpointBubble }) {
  const [restoring, setRestoring] = useState(false);
  const [outcome, setOutcome] = useState<"ok" | "error" | null>(null);
  const [error, setError] = useState<string | null>(null);

  const onRestore = useCallback(async () => {
    if (restoring) return;
    setRestoring(true);
    setError(null);
    try {
      const res = await fetch(
        `/api/chat-rail/checkpoints/${encodeURIComponent(bubble.checkpointId)}/restore`,
        { method: "POST", headers: { "content-type": "application/json" } },
      );
      if (!res.ok) {
        const body = await res.text();
        throw new Error(`HTTP ${res.status}: ${body || res.statusText}`);
      }
      setOutcome("ok");
    } catch (e) {
      setOutcome("error");
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setRestoring(false);
    }
  }, [bubble.checkpointId, restoring]);

  // The `restored` status renders as a full-width banner — operator must see
  // that subsequent in-flight messages have been rolled back. `created` and
  // `restore_failed` stay compact inline affordances.
  if (bubble.status === "restored") {
    return (
      <div className="w-full self-stretch">
        <div className="flex items-center gap-2 px-3 py-2 my-1 rounded-md border border-gold/40 bg-gold-bg text-gold text-[12px]">
          <span aria-hidden>↻</span>
          <span className="font-medium">Rolled back to checkpoint</span>
          <code className="font-mono text-[11px] opacity-80 truncate max-w-[180px]">
            {bubble.checkpointId}
          </code>
          <span className="ml-auto text-[11px] opacity-70">
            Messages above this point are hidden.
          </span>
        </div>
      </div>
    );
  }

  const label =
    bubble.status === "restore_failed" ? "Restore failed" : "Checkpoint";
  const tone =
    bubble.status === "restore_failed"
      ? "border-danger/40 bg-danger/10 text-danger"
      : "border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-400";

  return (
    <div className="min-w-0 max-w-[92%] self-start">
      <div
        className={`inline-flex items-center gap-2 px-2 py-1 rounded-md border text-[12px] ${tone}`}
      >
        <span aria-hidden>◷</span>
        <span className="font-medium">{label}</span>
        <code className="font-mono text-[11px] opacity-70 truncate max-w-[160px]">
          {bubble.checkpointId}
        </code>
        {bubble.status === "created" && (
          <button
            type="button"
            onClick={onRestore}
            disabled={restoring || outcome === "ok"}
            className="ml-1 px-1.5 py-0.5 rounded border border-current text-[11px] hover:bg-amber-500/15 disabled:opacity-40"
            title="Rewind workspace to this checkpoint"
          >
            {restoring
              ? "Restoring…"
              : outcome === "ok"
                ? "Restored ✓"
                : "Rewind"}
          </button>
        )}
      </div>
      {error && (
        <div className="mt-1 text-[11px] text-danger">{error}</div>
      )}
      {bubble.message && (
        <div className="mt-1 text-[11px] text-text-3">{bubble.message}</div>
      )}
    </div>
  );
}
