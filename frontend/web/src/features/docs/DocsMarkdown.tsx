import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

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
            const inline = !className;
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
          pre: (p) => (
            <pre
              {...p}
              className="font-mono text-[12px] bg-surface-elev border border-border-soft rounded p-3 mb-3 overflow-x-auto text-text"
            />
          ),
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
