import { useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeSlug from "rehype-slug";

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
 * Renders an in-app docs page body matching the folio-dark prototype's
 * editorial-document treatment (see `docs/design/xvnwiki/docs/docs.css`):
 *  - serif H1 at 28-30px, H2 at 18-20px, H3 at 13-15px
 *  - body at 13.5-14.5px with relaxed line-height (~1.55-1.65)
 *  - body copy in `--text-2`; emphasis in `--text`
 *  - inline `<code>` on a tinted accent background; block code on
 *    `--surface-code` with a hairline border
 *  - tables, key-value lists, and tabular nums all picked up via the
 *    existing dashboard tokens; no new tokens introduced here.
 *
 * No external network fetch — `react-markdown` runs on the baked string
 * from `/api/docs/page/:slug`.
 */
export function DocsMarkdown({ body }: { body: string }) {
  return (
    <div className="docs-markdown text-text text-[14px] leading-[1.65]">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeSlug]}
        components={{
          h1: (p) => (
            <h1
              {...p}
              className="font-sans font-semibold text-[28px] tracking-tight mb-4 mt-0"
            />
          ),
          h2: (p) => (
            <h2
              {...p}
              className="font-sans font-semibold text-[20px] tracking-tight mt-8 mb-2"
            />
          ),
          h3: (p) => (
            <h3
              {...p}
              className="font-medium text-[15px] tracking-tight mt-5 mb-1.5 text-text"
            />
          ),
          p: (p) => <p {...p} className="mb-4 text-text-2" />,
          ul: (p) => (
            <ul {...p} className="list-disc pl-5 mb-4 space-y-1.5 text-text-2" />
          ),
          ol: (p) => (
            <ol
              {...p}
              className="list-decimal pl-5 mb-4 space-y-1.5 text-text-2"
            />
          ),
          code: ({ className, children, ...rest }) => {
            const lang = langFromClass(className);
            // Inline code: no className or no language- prefix
            const inline = !lang;
            return inline ? (
              // Inline code: tinted accent background (subtle), thin
              // border, monospace at 12.5px. Matches prototype's
              // `--inline-code-bg` token treatment.
              <code
                {...rest}
                className="font-mono text-[12.5px] px-1.5 py-0.5 rounded-sm bg-gold/[0.06] border border-border-soft text-text whitespace-nowrap"
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
          // Tables match the prototype's editorial treatment: muted
          // uppercase header row, hairline separators, hover surface
          // wash. No outer rounded border — the table sits in body flow.
          table: (p) => (
            <table
              {...p}
              className="w-full text-[12.5px] mb-4 border-collapse text-text-2"
            />
          ),
          th: (p) => (
            <th
              {...p}
              className="text-left border-b border-border px-3 py-1.5 font-semibold text-text-2 text-[11.5px] uppercase tracking-wider bg-surface-elev/40"
            />
          ),
          td: (p) => (
            <td
              {...p}
              className="border-b border-border-soft px-3 py-2 align-top"
            />
          ),
          blockquote: (p) => (
            <blockquote
              {...p}
              className="border-l-2 border-gold/40 pl-4 my-4 text-text-2 font-medium"
            />
          ),
          hr: (p) => <hr {...p} className="my-7 border-border-soft" />,
          strong: (p) => <strong {...p} className="text-text font-semibold" />,
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
