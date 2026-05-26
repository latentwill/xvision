import { useCallback, useState } from "react";

import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

import { ContentBlockView } from "@/components/chat/ContentBlockView";

import type { Bubble, CheckpointBubble, RenderableBlock, Tool } from "./types";

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
  const tone = !ok
    ? "border-danger/40 bg-danger/10 text-danger"
    : pending
      ? "border-border-soft bg-surface-2/60 text-text-2"
      : "border-success/40 bg-success/10 text-success";
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
      <span className="font-mono truncate">{label}</span>
      {status && (
        <span className="text-text-3 truncate" title={status}>
          · {status}
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
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        pre: ({ children }: any) => (
          <pre className="font-mono text-[12px] bg-surface-2/70 p-2 rounded my-1.5 overflow-x-auto">
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
