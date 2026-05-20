import { useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

/** Extract the language token from a `language-X` className */
function langFromClass(className?: string): string | null {
  if (!className) return null;
  const match = className.match(/language-(\S+)/);
  return match ? match[1] : null;
}

/** Flatten React children to a plain string (handles nested arrays/elements) */
function childrenToText(children: React.ReactNode): string {
  if (typeof children === "string") return children;
  if (typeof children === "number") return String(children);
  if (Array.isArray(children)) return children.map(childrenToText).join("");
  if (children != null && typeof children === "object" && "props" in children) {
    return childrenToText((children as React.ReactElement).props.children);
  }
  return "";
}

function CodeCopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);

  function handleCopy() {
    if (!navigator.clipboard) return;
    navigator.clipboard.writeText(text).then(
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      },
      () => {
        // silently no-op on failure
      },
    );
  }

  return (
    <button
      onClick={handleCopy}
      className="font-mono text-[11px] border border-border-soft text-text-3 rounded px-1.5 py-0.5 hover:text-text hover:border-border transition-colors leading-none"
      aria-label={copied ? "Copied" : "Copy code"}
    >
      {copied ? "Copied" : "Copy"}
    </button>
  );
}

/**
 * Renders an in-app docs page body with the same styling treatment
 * as the rest of the dashboard — muted palette, monospace for code,
 * tabular nums for tables. No external network fetch: react-markdown
 * runs on the baked string from `/api/docs/page/:slug`.
 */
export function DocsMarkdown({ body }: { body: string }) {
  return (
    <div className="docs-markdown text-text text-[14px] leading-relaxed">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          h1: (p) => (
            <h1
              {...p}
              className="font-serif font-medium text-[28px] tracking-tight mb-3"
            />
          ),
          h2: (p) => (
            <h2
              {...p}
              className="font-serif font-medium text-[20px] tracking-tight mt-6 mb-2"
            />
          ),
          h3: (p) => (
            <h3
              {...p}
              className="font-medium text-[15px] tracking-tight mt-4 mb-1.5 text-text"
            />
          ),
          p: (p) => <p {...p} className="mb-3 text-text-2" />,
          ul: (p) => (
            <ul {...p} className="list-disc pl-5 mb-3 space-y-1 text-text-2" />
          ),
          ol: (p) => (
            <ol
              {...p}
              className="list-decimal pl-5 mb-3 space-y-1 text-text-2"
            />
          ),
          code: ({ className, children, ...rest }) => {
            const lang = langFromClass(className);
            // Inline code: no className or no language- prefix
            const inline = !lang;
            return inline ? (
              <code
                {...rest}
                className="font-mono text-[12px] px-1 py-0.5 rounded bg-surface-elev border border-border-soft text-text"
              >
                {children}
              </code>
            ) : (
              <code
                {...rest}
                className="font-mono text-[12px] text-text"
              >
                {children}
              </code>
            );
          },
          pre: ({ children, ...rest }) => {
            // Extract language and raw text from the nested <code> element
            let lang: string | null = null;
            let rawText = "";

            if (
              children != null &&
              typeof children === "object" &&
              "props" in (children as object)
            ) {
              const codeEl = children as React.ReactElement<{
                className?: string;
                children?: React.ReactNode;
              }>;
              lang = langFromClass(codeEl.props.className);
              rawText = childrenToText(codeEl.props.children);
            }

            return (
              <div className="relative mb-3 group">
                <pre
                  {...rest}
                  className="font-mono text-[12px] bg-surface-elev border border-border-soft rounded p-3 overflow-x-auto text-text"
                >
                  {children}
                </pre>
                <div className="absolute top-2 right-2 flex items-center gap-1.5">
                  {lang && (
                    <span
                      className="font-mono text-[11px] border border-border-soft text-text-3 rounded px-1.5 py-0.5 leading-none select-none"
                      data-testid="code-lang-badge"
                    >
                      {lang}
                    </span>
                  )}
                  <CodeCopyButton text={rawText} />
                </div>
              </div>
            );
          },
          table: (p) => (
            <table
              {...p}
              className="w-full text-[13px] mb-3 border-collapse text-text-2"
            />
          ),
          th: (p) => (
            <th
              {...p}
              className="text-left border-b border-border px-2 py-1 font-medium text-text"
            />
          ),
          td: (p) => (
            <td
              {...p}
              className="border-b border-border-soft px-2 py-1 align-top"
            />
          ),
          a: (p) => (
            <a
              {...p}
              className="text-gold hover:underline"
              target={p.href?.startsWith("http") ? "_blank" : undefined}
              rel={
                p.href?.startsWith("http") ? "noopener noreferrer" : undefined
              }
            />
          ),
        }}
      >
        {body}
      </ReactMarkdown>
    </div>
  );
}
