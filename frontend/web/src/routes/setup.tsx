import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";

import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";

import { streamChat, type WizardEvent } from "@/api/wizard";
import { ApiError } from "@/api/client";

// One bubble in the chat thread. Assistant bubbles accumulate text from
// `WizardEvent::Token` events; tool round-trips render inline as chips
// inside the assistant bubble that produced them.
type AssistantBubble = {
  role: "assistant";
  text: string;
  tools: { call: string; ok: boolean; summary: string }[];
};
type UserBubble = { role: "user"; text: string };
type Bubble = UserBubble | AssistantBubble;

export function SetupRoute() {
  const [bubbles, setBubbles] = useState<Bubble[]>([]);
  const [input, setInput] = useState("");
  const [isStreaming, setIsStreaming] = useState(false);
  const [draftId, setDraftId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  // Cancel any in-flight stream on unmount so the server-side WizardLoop
  // exits cleanly when the user navigates away mid-turn.
  useEffect(() => () => abortRef.current?.abort(), []);

  async function send() {
    if (!input.trim() || isStreaming) return;
    setError(null);
    const userText = input.trim();
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
      for await (const ev of streamChat({ message: userText }, ctrl.signal)) {
        applyEvent(setBubbles, setDraftId, ev);
      }
    } catch (e) {
      if (e instanceof ApiError) {
        setError(`${e.code}: ${e.message}`);
      } else if ((e as Error).name === "AbortError") {
        // user-initiated; no message
      } else {
        setError(String(e));
      }
    } finally {
      setIsStreaming(false);
      abortRef.current = null;
    }
  }

  return (
    <>
      <Topbar
        title="Setup"
        sub={
          isStreaming
            ? "Streaming…"
            : draftId
              ? "Draft ready"
              : "Tell the wizard what you want to build"
        }
      />

      <Card className="px-6 py-5 mb-3">
        <div className="text-text-2 text-[14px] leading-snug max-w-prose">
          The setup agent walks you from a plain-English description to a
          validated <span className="text-text">StrategyBundle</span> ready to
          backtest. Try:{" "}
          <span className="text-text font-mono">
            "Buys dips when the trend is up"
          </span>{" "}
          or <span className="text-text font-mono">"Mean reversion on BTC"</span>.
        </div>
      </Card>

      <Card className="p-0 overflow-hidden">
        <Thread bubbles={bubbles} streaming={isStreaming} />
        {error && (
          <div className="px-5 py-3 border-t border-border text-rose-300 dark:text-rose-300 text-[13px]">
            {error}
          </div>
        )}
        {draftId && (
          <div className="px-5 py-3 border-t border-border bg-surface-2/40 flex items-center justify-between">
            <div className="text-[13px] text-text-2">
              Draft <span className="font-mono text-text">{draftId}</span> is
              tracked.
            </div>
            <Link
              to={`/authoring/${draftId}`}
              className="text-[13px] text-blue-300 hover:underline"
            >
              Open in Inspector →
            </Link>
          </div>
        )}
        <Composer
          value={input}
          onChange={setInput}
          onSubmit={send}
          disabled={isStreaming}
        />
      </Card>
    </>
  );
}

function applyEvent(
  setBubbles: React.Dispatch<React.SetStateAction<Bubble[]>>,
  setDraftId: React.Dispatch<React.SetStateAction<string | null>>,
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
      // Match the most recent same-named call (matches the server-side
      // pairing order; the loop emits ToolCall + ToolResult adjacently).
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
    } else if (ev.type === "done") {
      if (ev.draft_id) setDraftId(ev.draft_id);
    } else if (ev.type === "error") {
      a.text = a.text
        ? `${a.text}\n\n[stream error: ${ev.message}]`
        : `[stream error: ${ev.message}]`;
    }
    next[next.length - 1] = a;
    return next;
  });
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

function Thread({
  bubbles,
  streaming,
}: {
  bubbles: Bubble[];
  streaming: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    ref.current?.scrollTo({
      top: ref.current.scrollHeight,
      behavior: "smooth",
    });
  }, [bubbles, streaming]);
  return (
    <div
      ref={ref}
      className="max-h-[60vh] overflow-y-auto px-5 py-4 flex flex-col gap-3"
    >
      {bubbles.length === 0 ? (
        <div className="text-text-3 italic font-serif text-[15px] text-center py-6">
          Start by describing the strategy in your own words.
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
      <div className="self-end max-w-[85%]">
        <div className="bg-blue-500/10 dark:bg-blue-400/10 border border-blue-500/30 dark:border-blue-400/30 rounded-md px-3 py-2 text-[14px] whitespace-pre-wrap leading-snug">
          {b.text}
        </div>
      </div>
    );
  }
  return (
    <div className="self-start max-w-[85%]">
      <div className="bg-surface-2/60 border border-border rounded-md px-3 py-2 text-[14px] whitespace-pre-wrap leading-snug">
        {b.text || <span className="text-text-3 italic">thinking…</span>}
      </div>
      {b.tools.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {b.tools.map((t, i) => (
            <Pill key={i} tone={t.ok ? "info" : "danger"}>
              <span className="font-mono">{t.call}</span>
              {t.summary && <span className="text-text-3"> · {t.summary}</span>}
            </Pill>
          ))}
        </div>
      )}
    </div>
  );
}

function Composer({
  value,
  onChange,
  onSubmit,
  disabled,
}: {
  value: string;
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
      className="border-t border-border px-4 py-3 flex gap-2 bg-surface-2/30"
    >
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={disabled}
        placeholder={
          disabled ? "Streaming…" : "Describe your strategy or ask the wizard…"
        }
        className="flex-1 bg-transparent border border-border rounded-md px-3 py-2 text-[14px] placeholder:text-text-3 focus:outline-none focus:ring-1 focus:ring-text-2"
      />
      <button
        type="submit"
        disabled={disabled || !value.trim()}
        className="px-4 py-2 rounded-md text-[13px] border border-border bg-surface-2/60 hover:bg-surface-2 disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {disabled ? "…" : "Send"}
      </button>
    </form>
  );
}
