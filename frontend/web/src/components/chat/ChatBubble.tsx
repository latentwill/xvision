import type { ReactNode } from "react";

import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

import { ContentBlockView } from "@/components/chat/ContentBlockView";
import { Pill } from "@/components/primitives/Pill";

import type { Bubble, RenderableBlock, Tool } from "./types";

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

  const showDots = isStreaming && isLast;
  const hasRenderableBlocks = bubble.blocks.some(
    (block) => block.kind !== "text" || block.text.length > 0,
  );
  const narratives = bubble.tools
    .map((t, i) => ({ i, n: toolLogLine(t) }))
    .filter(
      (x): x is { i: number; n: { ok: boolean; content: ReactNode } } =>
        x.n !== null,
    );

  return (
    <div className="min-w-0 max-w-[92%] self-start">
      <div className="break-words bg-surface-2/60 border border-border rounded-md px-2.5 py-1.5 text-[13px] leading-snug">
        {hasRenderableBlocks ? (
          <>
            <ContentBlocksView blocks={bubble.blocks} />
            {showDots && <TypingDots inline />}
          </>
        ) : showDots ? (
          <TypingDots />
        ) : (
          <span className="text-text-3 font-medium">thinking...</span>
        )}
      </div>
      {narratives.length > 0 && (
        <div className="mt-1.5 flex flex-col gap-1">
          {narratives.map(({ i, n }) => (
            <div
              key={`narr-${i}`}
              className={`text-[12px] flex items-start gap-1.5 ${
                n.ok ? "text-info" : "text-danger"
              }`}
            >
              <span className="leading-[1.4] flex-shrink-0">
                {n.ok ? "+" : "!"}
              </span>
              <span className="leading-[1.4]">{n.content}</span>
            </div>
          ))}
        </div>
      )}
      {bubble.tools.length > 0 && (
        <div className="mt-1.5 flex flex-wrap gap-1 opacity-60">
          {bubble.tools.map((t, i) => (
            <Pill key={i} tone={t.ok ? "info" : "danger"}>
              {t.pending && (
                <span
                  className="inline-block w-2 h-2 mr-1 border border-current border-t-transparent rounded-full animate-spin align-middle"
                  aria-label="running"
                />
              )}
              <span className="font-mono">{t.call}</span>
              {t.summary && (
                <span className="text-text-3"> - {t.summary}</span>
              )}
            </Pill>
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

function toolLogLine(
  t: Tool,
): { ok: boolean; content: ReactNode } | null {
  const args = (t.args ?? {}) as Record<string, unknown>;
  const result = (t.result ?? {}) as Record<string, unknown>;
  const errorMsg =
    typeof result.error === "string" ? result.error : undefined;
  if (errorMsg || !t.ok) {
    const detail = errorMsg ?? t.resultSummary ?? t.summary ?? "Tool failed";
    return {
      ok: false,
      content: (
        <>
          {friendlyVerb(t.call)} failed: <span>{detail}</span>
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
            <code className="font-mono text-text-2">{String(arg)}</code>...
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
      const agentResult = result["agent"];
      const createdAgent =
        typeof agentResult === "object" &&
        agentResult !== null &&
        "agent_id" in agentResult;
      if (!t.pending && !id && args["name"] == null) {
        return {
          ok: true,
          content: (
            <>
              create_strategy completed
              {t.resultSummary ? (
                <span className="text-text-2">: {t.resultSummary}</span>
              ) : null}
            </>
          ),
        };
      }
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
            {createdAgent ? <> and attached a trader agent</> : null}
          </>
        ),
      };
    }
    case "create_strategy_agent": {
      const role = String(args["role"] ?? "trader");
      const agentId =
        typeof result["agent_id"] === "string" ? result["agent_id"] : "";
      return {
        ok: true,
        content: t.pending ? (
          <>
            Creating <code className="font-mono text-text">{role}</code> agent...
          </>
        ) : agentId ? (
          <>
            Attached <code className="font-mono text-text">{role}</code> agent{" "}
            <code className="font-mono text-text-2">{agentId}</code>
          </>
        ) : (
          <>Attached strategy agent</>
        ),
      };
    }
    case "attach_agent": {
      const role = String(args["role"] ?? "trader");
      return {
        ok: true,
        content: t.pending ? (
          <>
            Attaching <code className="font-mono text-text">{role}</code> agent...
          </>
        ) : (
          <>
            Attached <code className="font-mono text-text">{role}</code> agent
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
            {t.pending ? "Calling" : "Set"}{" "}
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
            Updating <code className="font-mono text-text">{slot}</code>...
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
    case "update_manifest": {
      const updated = Array.isArray(result["updated"])
        ? (result["updated"] as string[]).join(", ")
        : "";
      return {
        ok: true,
        content: t.pending ? (
          <>Updating manifest...</>
        ) : updated ? (
          <>Updated manifest: {updated}</>
        ) : (
          <>Updated manifest</>
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
            Eval run <code className="font-mono text-text">{runId}</code>
            {t.resultSummary ? (
              <span className="text-text-2"> ({t.resultSummary})</span>
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
              <span className="text-text-2">{t.summary}</span>...
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
    case "create_strategy_agent":
      return "Create agent";
    case "attach_agent":
      return "Attach agent";
    case "set_mechanical_param":
      return "Set parameter";
    case "set_risk_config":
      return "Set risk";
    case "validate_draft":
      return "Validate";
    case "update_slot":
      return "Update slot";
    case "update_manifest":
      return "Update manifest";
    default:
      return call;
  }
}
