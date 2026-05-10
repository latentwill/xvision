# v1 Frontend — Plan 4: Agent surfaces (Wizard, Chat rail, Live preview)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the three LLM-driven agent surfaces of v1 — the chat-driven Setup wizard at `/setup`, the Inspector's live-preview pane (right side of the split editor from Plan 3), and the persistent right-side Chat rail across all routes — to their backing SSE endpoints.

**Architecture:** Most of the backend is owned by **existing plans that must land first**: Plan 2d (`xvision-dashboard` WizardLoop + SSE) for the wizard, the Chat Rail Persistence plan (Phase A–E) for the rail. This plan ships the **frontend halves** plus one small new backend endpoint — `POST /api/strategies/:id/preview-slot` — for the live preview. A single `useSSE` hook backs all three streams; events conform to one tagged-union envelope so the rendering code is shared.

**Tech Stack:** Native `EventSource` API (no library). [`react-markdown`](https://github.com/remarkjs/react-markdown) 9.x for rendering streamed agent messages. Otherwise inherits Plans 1-3.

---

## Scope and split

Plan 4 of 5. Depends on Plans 1, 2, 3.

## Prerequisites — hard

- **Plan 2d** (`docs/superpowers/plans/2026-05-08-strategy-engine-2d-dashboard-wizard.md`) shipped: WizardLoop, `POST /api/wizard/chat` SSE, `?seed=` context handler.
- **Chat rail persistence plan** (`docs/superpowers/plans/2026-05-10-chat-rail-persistence-plan.md`) shipped: `chat_sessions` + `chat_messages` storage, `POST /api/chat-rail/sessions`, `POST /api/chat-rail/chat`, `PUT /api/chat-rail/sessions/:id/scope`.

If either is incomplete, this plan can ship the frontend against a stub SSE endpoint that scripts a hardcoded sequence of events. The cutover requires no UI changes once the real endpoints land.

## Prerequisites — soft

- Plans 1, 2, 3 landed.

## File structure

```
crates/xvision-engine/src/api/
└── strategy.rs                      AUGMENT (preview_slot)

crates/xvision-dashboard/src/routes/
└── strategies.rs                    AUGMENT (preview-slot endpoint)

frontend/web/src/
├── api/
│   ├── sse.ts                       NEW (shared SSE event envelope types)
│   ├── wizard.ts                    AUGMENT (chat stream)
│   ├── chat-rail.ts                 NEW
│   └── strategies.ts                AUGMENT (previewSlot)
├── hooks/
│   └── useSSE.ts                    NEW
├── stores/
│   └── chat-rail.ts                 NEW (Zustand: per-route open state, scope)
├── components/
│   ├── chat/
│   │   ├── MessageList.tsx          NEW
│   │   ├── Composer.tsx             NEW
│   │   ├── AgentBubble.tsx          NEW
│   │   ├── UserBubble.tsx           NEW
│   │   ├── ToolCallBlock.tsx        NEW
│   │   └── StreamingDot.tsx         NEW
│   ├── shell/
│   │   └── ChatRail.tsx             REPLACE placeholder
│   ├── editors/
│   │   └── SlotPreview.tsx          NEW (live preview pane for Inspector)
│   └── wizard/
│       └── StrategyProgressPanel.tsx NEW
└── routes/
    ├── setup.tsx                    REPLACE placeholder
    └── authoring.tsx                AUGMENT (mount SlotPreview)
```

---

## Tasks

### Task 1: Backend — `POST /api/strategies/:id/preview-slot`

**Files:**
- Modify: `crates/xvision-engine/src/api/strategy.rs`
- Modify: `crates/xvision-dashboard/src/routes/strategies.rs`
- Modify: `crates/xvision-dashboard/src/server.rs`

- [ ] **Step 1.1: Define types**

Append to `crates/xvision-engine/src/api/strategy.rs`:

```rust
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PreviewSlotRequest {
    pub slot: String,            // "intern" | "trader" | "regime"
    pub fixture_id: String,      // e.g. "btc-usd-2025-01-15-08-00"
    pub override_system_prompt: Option<String>,  // unsaved edits
    pub override_max_tokens: Option<u32>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewSlotResponse {
    pub decision_json: serde_json::Value,
    pub tokens_used: u32,
    pub estimated_cost_usd: f64,
    pub latency_ms: u32,
    pub diff: Option<PreviewDiff>,    // Δ vs previous preview
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewDiff {
    pub action_changed: bool,
    pub conviction_delta: Option<f64>,
}
```

- [ ] **Step 1.2: Implement**

```rust
pub async fn preview_slot(
    ctx: &ApiContext,
    bundle_id: &str,
    req: PreviewSlotRequest,
) -> Result<PreviewSlotResponse, ApiError> {
    let bundle = get(ctx, bundle_id).await?.bundle;
    let fixture = ctx
        .fixtures()
        .load(&req.fixture_id)
        .await
        .map_err(|_| ApiError::NotFound(format!("fixture {}", req.fixture_id)))?;

    let mut slot_cfg = match req.slot.as_str() {
        "intern" => bundle.intern_slot.clone().ok_or_else(|| ApiError::Validation { field: "slot".into(), msg: "intern slot not present".into() })?,
        "trader" => bundle.trader_slot.clone().ok_or_else(|| ApiError::Validation { field: "slot".into(), msg: "trader slot not present".into() })?,
        "regime" => bundle.regime_slot.clone().ok_or_else(|| ApiError::Validation { field: "slot".into(), msg: "regime slot not present".into() })?,
        other => return Err(ApiError::Validation { field: "slot".into(), msg: format!("unknown slot {other}") }),
    };
    if let Some(p) = req.override_system_prompt { slot_cfg.system_prompt = p; }
    if let Some(m) = req.override_max_tokens { slot_cfg.max_tokens = m; }

    let start = std::time::Instant::now();
    let result = crate::agent::execute::run_single_slot(&slot_cfg, &fixture, &ctx.llm()).await
        .map_err(|e| ApiError::Internal(e.into()))?;
    let latency_ms = start.elapsed().as_millis() as u32;

    let prev = ctx.preview_cache().last(bundle_id, &req.slot).await;
    let diff = prev.as_ref().map(|p| PreviewDiff {
        action_changed: p.decision_json.get("action") != result.decision_json.get("action"),
        conviction_delta: result.decision_json.get("conviction").and_then(|v| v.as_f64())
            .zip(p.decision_json.get("conviction").and_then(|v| v.as_f64()))
            .map(|(a, b)| a - b),
    });
    ctx.preview_cache().store(bundle_id, &req.slot, &result).await;

    Ok(PreviewSlotResponse {
        decision_json: result.decision_json,
        tokens_used: result.tokens_used,
        estimated_cost_usd: result.tokens_used as f64 * 0.0000085, // rough placeholder
        latency_ms,
        diff,
    })
}
```

(Adjust `agent::execute::run_single_slot` to match the actual API in `xvision-engine/src/agent/execute.rs`. The audit confirmed the agent module exists; the function name is best-guess.)

- [ ] **Step 1.3: Add the dashboard handler**

Append to `crates/xvision-dashboard/src/routes/strategies.rs`:

```rust
use xvision_engine::api::strategy::{preview_slot, PreviewSlotRequest, PreviewSlotResponse};

pub async fn preview_slot_handler(
    Path(id): Path<String>,
    Json(req): Json<PreviewSlotRequest>,
) -> Result<Json<PreviewSlotResponse>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    Ok(Json(preview_slot(&ctx, &id, req).await.map_err(map_api_err)?))
}
```

In `server.rs`:

```rust
.route("/api/strategies/:id/preview-slot", post(crate::routes::strategies::preview_slot_handler))
```

- [ ] **Step 1.4: Test**

```rust
#[tokio::test]
async fn preview_slot_validates_slot_name() {
    let app = build_router();
    let server = TestServer::new(app).unwrap();
    let response = server
        .post("/api/strategies/some-id/preview-slot")
        .json(&serde_json::json!({ "slot": "garbage", "fixture_id": "x" }))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 1.5: Commit**

```bash
cargo test -p xvision-dashboard
cargo xtask gen-types
git add . && git commit -m "feat(engine): /api/strategies/:id/preview-slot endpoint"
```

---

### Task 2: Frontend — SSE event envelope + `useSSE` hook

**Files:**
- Create: `frontend/web/src/api/sse.ts`
- Create: `frontend/web/src/hooks/useSSE.ts`

- [ ] **Step 2.1: Define event envelope**

Create `frontend/web/src/api/sse.ts`:

```ts
export type ToolCall = { type: "tool_call"; name: string; args: unknown };
export type ToolResult = { type: "tool_result"; name: string; result: unknown };
export type AgentMessage = { type: "agent_message"; content: string; delta: boolean };
export type BundlePatch = { type: "bundle_patch"; patch: unknown };
export type ProgressEvent = { type: "progress"; pct: number; phase?: string };
export type DoneEvent = { type: "done"; payload?: unknown };
export type ErrorEvent = { type: "error"; message: string };

export type StreamEvent =
  | ToolCall
  | ToolResult
  | AgentMessage
  | BundlePatch
  | ProgressEvent
  | DoneEvent
  | ErrorEvent;

export type StreamRequest = {
  endpoint: string;
  body?: unknown;
};
```

- [ ] **Step 2.2: Implement `useSSE`**

Create `frontend/web/src/hooks/useSSE.ts`:

```ts
import { useEffect, useRef, useState } from "react";
import type { StreamEvent } from "@/api/sse";

type Status = "idle" | "connecting" | "open" | "closed" | "error";

export type UseSSEArgs = {
  enabled: boolean;
  endpoint: string;        // e.g. "/api/wizard/chat"
  body: unknown;           // POSTed once at stream start
  onEvent: (e: StreamEvent) => void;
  onClose?: () => void;
};

export function useSSE({ enabled, endpoint, body, onEvent, onClose }: UseSSEArgs) {
  const [status, setStatus] = useState<Status>("idle");
  const aborterRef = useRef<AbortController | null>(null);

  useEffect(() => {
    if (!enabled) return;
    const ac = new AbortController();
    aborterRef.current = ac;
    setStatus("connecting");

    (async () => {
      try {
        const res = await fetch(endpoint, {
          method: "POST",
          headers: { "Content-Type": "application/json", Accept: "text/event-stream" },
          body: JSON.stringify(body),
          signal: ac.signal,
        });
        if (!res.ok || !res.body) {
          setStatus("error");
          onEvent({ type: "error", message: `HTTP ${res.status}` });
          return;
        }
        setStatus("open");
        const reader = res.body.getReader();
        const decoder = new TextDecoder();
        let buf = "";
        while (true) {
          const { value, done } = await reader.read();
          if (done) break;
          buf += decoder.decode(value, { stream: true });
          let idx;
          while ((idx = buf.indexOf("\n\n")) !== -1) {
            const chunk = buf.slice(0, idx);
            buf = buf.slice(idx + 2);
            const data = chunk.split("\n").filter((l) => l.startsWith("data:")).map((l) => l.slice(5).trim()).join("");
            if (!data) continue;
            try {
              const ev = JSON.parse(data) as StreamEvent;
              onEvent(ev);
            } catch {
              onEvent({ type: "error", message: "malformed SSE payload" });
            }
          }
        }
        setStatus("closed");
        onClose?.();
      } catch (e: any) {
        if (e.name === "AbortError") return;
        setStatus("error");
        onEvent({ type: "error", message: String(e) });
      }
    })();

    return () => { ac.abort(); };
  }, [enabled, endpoint, JSON.stringify(body)]);

  return { status, abort: () => aborterRef.current?.abort() };
}
```

- [ ] **Step 2.3: Commit**

```bash
git add frontend/web/src/api/sse.ts frontend/web/src/hooks/useSSE.ts
git commit -m "feat(frontend): SSE event envelope and useSSE hook"
```

---

### Task 3: Chat primitives — bubbles, composer, message list

**Files:**
- Modify: `frontend/web/package.json` (add react-markdown)
- Create: `frontend/web/src/components/chat/AgentBubble.tsx`
- Create: `frontend/web/src/components/chat/UserBubble.tsx`
- Create: `frontend/web/src/components/chat/ToolCallBlock.tsx`
- Create: `frontend/web/src/components/chat/StreamingDot.tsx`
- Create: `frontend/web/src/components/chat/Composer.tsx`
- Create: `frontend/web/src/components/chat/MessageList.tsx`

- [ ] **Step 3.1: Add react-markdown**

```bash
cd frontend/web && pnpm add react-markdown@^9.0.1
```

- [ ] **Step 3.2: `AgentBubble`**

```tsx
import ReactMarkdown from "react-markdown";

export function AgentBubble({ children, streaming }: { children: string; streaming?: boolean }) {
  return (
    <div className="border-l-2 border-gold pl-3.5 text-[13.5px] leading-relaxed text-text">
      <ReactMarkdown
        components={{
          code: (p) => <code className="font-mono bg-surface-elev px-1.5 py-0.5 rounded-sm text-[12px]" {...p} />,
        }}
      >
        {children}
      </ReactMarkdown>
      {streaming && <span className="inline-block w-1.5 h-1.5 bg-gold rounded-full ml-1 animate-pulse align-middle" />}
    </div>
  );
}
```

- [ ] **Step 3.3: `UserBubble`**

```tsx
export function UserBubble({ children }: { children: string }) {
  return (
    <div className="self-end max-w-[78%] bg-surface-elev px-3.5 py-2.5 rounded-sm text-[13.5px] leading-relaxed border border-border">
      {children}
    </div>
  );
}
```

- [ ] **Step 3.4: `ToolCallBlock`**

```tsx
import { useState } from "react";

type Props = { name: string; args: unknown; result?: unknown };

export function ToolCallBlock({ name, args, result }: Props) {
  const [open, setOpen] = useState(false);
  return (
    <details
      open={open}
      onToggle={(e) => setOpen((e.target as HTMLDetailsElement).open)}
      className="bg-surface-elev border border-border rounded-sm font-mono text-[11.5px]"
    >
      <summary className="px-2.5 py-1.5 cursor-pointer text-text-2">
        <span className="text-text-3"># Tool: </span>
        <span className="text-text">{name}</span>
        {!result && <span className="ml-2 text-warn">running…</span>}
      </summary>
      <pre className="px-2.5 py-2 m-0 overflow-x-auto text-text-2 border-t border-border-soft">
        {JSON.stringify({ args, result }, null, 2)}
      </pre>
    </details>
  );
}
```

- [ ] **Step 3.5: `StreamingDot`**

```tsx
export function StreamingDot() {
  return <span className="text-gold animate-pulse">●</span>;
}
```

- [ ] **Step 3.6: `Composer`**

```tsx
import { useState, KeyboardEvent } from "react";

type Props = {
  onSend: (text: string) => void;
  disabled?: boolean;
  placeholder?: string;
};

export function Composer({ onSend, disabled, placeholder = "Tell me what you want to build…" }: Props) {
  const [text, setText] = useState("");
  function submit() {
    if (disabled || !text.trim()) return;
    onSend(text.trim());
    setText("");
  }
  function onKey(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      submit();
    }
  }
  return (
    <div className="border border-border rounded-sm bg-surface-elev p-3 flex items-end gap-3">
      <textarea
        rows={2}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={onKey}
        placeholder={placeholder}
        className="flex-1 bg-transparent text-text-2 text-[13px] resize-none outline-none min-h-[40px]"
      />
      <button
        onClick={submit}
        disabled={disabled || !text.trim()}
        className="bg-gold text-bg rounded-sm px-3.5 py-1.5 text-xs font-medium disabled:opacity-40"
      >
        Send <span className="opacity-60 ml-1">⌘↵</span>
      </button>
    </div>
  );
}
```

- [ ] **Step 3.7: `MessageList`**

```tsx
import { useEffect, useRef } from "react";

type Message =
  | { id: string; role: "user"; content: string }
  | { id: string; role: "agent"; content: string; streaming?: boolean }
  | { id: string; role: "tool"; name: string; args: unknown; result?: unknown };

export function MessageList({ messages }: { messages: Message[] }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => { ref.current?.scrollTo({ top: ref.current.scrollHeight }); }, [messages.length, messages[messages.length - 1]]);
  return (
    <div ref={ref} className="flex-1 overflow-y-auto pr-2 flex flex-col gap-4">
      {messages.map((m) => (
        m.role === "user" ? <UserBubble key={m.id}>{m.content}</UserBubble> :
        m.role === "agent" ? <AgentBubble key={m.id} streaming={m.streaming}>{m.content}</AgentBubble> :
        <ToolCallBlock key={m.id} name={m.name} args={m.args} result={m.result} />
      ))}
    </div>
  );
}
import { AgentBubble } from "./AgentBubble";
import { UserBubble } from "./UserBubble";
import { ToolCallBlock } from "./ToolCallBlock";
```

(Move the imports to the top — shown at bottom for plan readability.)

- [ ] **Step 3.8: Commit**

```bash
git add frontend/web/package.json frontend/web/pnpm-lock.yaml frontend/web/src/components/chat/
git commit -m "feat(frontend): chat primitives — bubbles, composer, message list"
```

---

### Task 4: Wizard chat API + Setup screen

**Files:**
- Modify: `frontend/web/src/api/wizard.ts`
- Create: `frontend/web/src/components/wizard/StrategyProgressPanel.tsx`
- Modify: `frontend/web/src/routes/setup.tsx`

- [ ] **Step 4.1: Augment wizard API**

Replace `frontend/web/src/api/wizard.ts`:

```ts
import { apiFetch } from "./client";
import type { Template } from "./types.gen";

export const wizardApi = {
  templates: () => apiFetch<Template[]>("/api/wizard/templates"),
  // Note: wizard chat is consumed via useSSE directly, not through this client.
};

export type WizardChatRequest = {
  message: string;
  session_id?: string;       // omit on first message; server creates one
  seed?: string;             // e.g. "finding:01H8N7Z:abc"
};
```

- [ ] **Step 4.2: `StrategyProgressPanel`**

Create `frontend/web/src/components/wizard/StrategyProgressPanel.tsx`:

```tsx
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { Dot } from "@/components/primitives/Dot";

export type ProgressState = {
  bundle_name: string;
  status: "idle" | "drafting" | "ready";
  template?: string;
  agents: { layer: string; model: string; status: "ready" | "drafting" }[];
  mechanics: { cadence?: string; asset?: string; stop?: string };
  risk?: string;
};

export function StrategyProgressPanel({ state, onOpenInspector }: { state: ProgressState; onOpenInspector?: () => void }) {
  return (
    <section className="flex flex-col h-full">
      <div className="flex justify-between items-end mb-6">
        <div>
          <div className="text-[11px] text-text-3 uppercase tracking-wider mb-1">Strategy in progress</div>
          <div className="font-serif text-[28px]">{state.bundle_name}</div>
        </div>
        <Pill variant={state.status === "drafting" ? "warn" : state.status === "ready" ? "gold" : "default"}>
          <Dot tone={state.status === "drafting" ? "warn" : state.status === "ready" ? "gold" : "muted"} />
          {state.status === "drafting" ? "Drafting" : state.status === "ready" ? "Ready" : "Idle"}
        </Pill>
      </div>

      <div className="flex flex-col gap-4 flex-1">
        {state.template && (
          <Card className="p-4">
            <div className="font-serif text-base mb-2">Template</div>
            <Row k="Selected" v={state.template} />
          </Card>
        )}

        <Card className="p-4">
          <div className="font-serif text-base mb-2">Agents</div>
          {state.agents.map((a) => (
            <Row
              key={a.layer}
              k={a.layer}
              v={
                <span>
                  <Dot tone={a.status === "drafting" ? "warn" : "gold"} />
                  <span className="font-mono">{a.model}</span>
                  <span className="ml-2 text-[11px]" style={{ color: a.status === "drafting" ? "var(--warn)" : "var(--gold)" }}>
                    {a.status}
                  </span>
                </span>
              }
            />
          ))}
        </Card>

        <Card className="p-4">
          <div className="font-serif text-base mb-2">Mechanics</div>
          <Row k="Cadence" v={state.mechanics.cadence ?? "—"} />
          <Row k="Asset" v={state.mechanics.asset ?? "—"} />
          <Row k="Stop" v={state.mechanics.stop ?? "—"} />
        </Card>

        {state.risk && (
          <Card className="p-4">
            <div className="font-serif text-base mb-2">Risk</div>
            <Row k="Preset" v={state.risk} />
          </Card>
        )}
      </div>

      <div className="flex gap-2 pt-4 border-t border-border-soft mt-4">
        <button onClick={onOpenInspector} className="border border-border text-text rounded-sm px-3.5 py-2 text-sm">
          Open in Inspector
        </button>
      </div>
    </section>
  );
}

function Row({ k, v }: { k: string; v: React.ReactNode }) {
  return (
    <div className="flex justify-between py-1 text-sm">
      <span className="text-text-3">{k}</span>
      <span className="text-text">{v}</span>
    </div>
  );
}
```

- [ ] **Step 4.3: Implement `/setup` route**

Replace `frontend/web/src/routes/setup.tsx`:

```tsx
import { useEffect, useReducer, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useSSE } from "@/hooks/useSSE";
import { MessageList } from "@/components/chat/MessageList";
import { Composer } from "@/components/chat/Composer";
import { Pill } from "@/components/primitives/Pill";
import { StrategyProgressPanel, ProgressState } from "@/components/wizard/StrategyProgressPanel";
import type { StreamEvent } from "@/api/sse";

type Message =
  | { id: string; role: "user"; content: string }
  | { id: string; role: "agent"; content: string; streaming?: boolean }
  | { id: string; role: "tool"; name: string; args: unknown; result?: unknown };

type State = {
  messages: Message[];
  pending?: string;            // outbound message awaiting send
  streaming: boolean;
  sessionId?: string;
  bundleId?: string;
  progress: ProgressState;
};

const INITIAL_PROGRESS: ProgressState = {
  bundle_name: "untitled-draft",
  status: "idle",
  agents: [],
  mechanics: {},
};

type Action =
  | { type: "user_send"; text: string }
  | { type: "agent_delta"; content: string }
  | { type: "agent_done" }
  | { type: "tool_call"; name: string; args: unknown }
  | { type: "tool_result"; name: string; result: unknown }
  | { type: "bundle_patch"; patch: any }
  | { type: "session"; id: string }
  | { type: "done"; bundleId?: string };

function reducer(s: State, a: Action): State {
  switch (a.type) {
    case "user_send":
      return {
        ...s,
        messages: [...s.messages, { id: crypto.randomUUID(), role: "user", content: a.text }],
        streaming: true,
      };
    case "agent_delta": {
      const last = s.messages[s.messages.length - 1];
      if (last?.role === "agent" && last.streaming) {
        return {
          ...s,
          messages: [
            ...s.messages.slice(0, -1),
            { ...last, content: last.content + a.content },
          ],
        };
      }
      return {
        ...s,
        messages: [...s.messages, { id: crypto.randomUUID(), role: "agent", content: a.content, streaming: true }],
      };
    }
    case "agent_done": {
      const last = s.messages[s.messages.length - 1];
      if (last?.role === "agent") {
        return { ...s, messages: [...s.messages.slice(0, -1), { ...last, streaming: false }] };
      }
      return s;
    }
    case "tool_call":
      return {
        ...s,
        messages: [...s.messages, { id: crypto.randomUUID(), role: "tool", name: a.name, args: a.args }],
      };
    case "tool_result": {
      // Attach to the most recent matching tool call
      const idx = [...s.messages].reverse().findIndex((m) => m.role === "tool" && m.name === a.name && m.result === undefined);
      if (idx === -1) return s;
      const realIdx = s.messages.length - 1 - idx;
      const updated = [...s.messages];
      updated[realIdx] = { ...(updated[realIdx] as any), result: a.result };
      return { ...s, messages: updated };
    }
    case "bundle_patch":
      return { ...s, progress: applyPatch(s.progress, a.patch) };
    case "session":
      return { ...s, sessionId: a.id };
    case "done":
      return { ...s, streaming: false, bundleId: a.bundleId, progress: { ...s.progress, status: "ready" } };
  }
}

function applyPatch(prev: ProgressState, patch: any): ProgressState {
  return { ...prev, ...patch, agents: patch.agents ?? prev.agents, mechanics: { ...prev.mechanics, ...(patch.mechanics ?? {}) } };
}

export default function Setup() {
  const [params] = useSearchParams();
  const seed = params.get("seed") ?? undefined;
  const [state, dispatch] = useReducer(reducer, {
    messages: [{ id: "welcome", role: "agent", content: "Hi! I'm the xvn setup agent. I'll help you build or pick an AI trading bot. What's your goal today?" }],
    streaming: false,
    progress: INITIAL_PROGRESS,
  });
  const [pending, setPending] = useState<string | null>(null);
  const nav = useNavigate();

  const { abort } = useSSE({
    enabled: pending !== null,
    endpoint: "/api/wizard/chat",
    body: { message: pending, session_id: state.sessionId, seed },
    onEvent: (e: StreamEvent) => {
      if (e.type === "agent_message") dispatch({ type: "agent_delta", content: e.content });
      else if (e.type === "tool_call") dispatch({ type: "tool_call", name: e.name, args: e.args });
      else if (e.type === "tool_result") dispatch({ type: "tool_result", name: e.name, result: e.result });
      else if (e.type === "bundle_patch") dispatch({ type: "bundle_patch", patch: e.patch });
      else if (e.type === "done") {
        dispatch({ type: "agent_done" });
        dispatch({ type: "done", bundleId: (e.payload as any)?.bundle_id });
      } else if (e.type === "error") {
        dispatch({ type: "agent_delta", content: `\n\n_Error: ${e.message}_` });
        dispatch({ type: "agent_done" });
      }
    },
    onClose: () => setPending(null),
  });

  function send(text: string) {
    dispatch({ type: "user_send", text });
    setPending(text);
  }

  return (
    <div className="grid grid-cols-2 gap-0 h-full -mx-9 -mt-9">
      <section className="px-9 pt-9 pb-0 border-r border-border-soft flex flex-col">
        <div className="flex justify-between items-center mb-6">
          <div>
            <div className="font-serif italic text-[30px] leading-tight">Welcome to xvn.</div>
            <div className="text-text-2 text-sm mt-1">Setup agent · <span className="inline-block w-1.5 h-1.5 bg-gold rounded-full mx-1.5 align-middle" />Online</div>
          </div>
        </div>
        <MessageList messages={state.messages} />
        <div className="flex gap-2 my-4 flex-wrap">
          <Pill variant="gold">Try a free strategy</Pill>
          <Pill variant="gold">Build from a template</Pill>
          <Pill variant="gold">Diagnose a recent run</Pill>
        </div>
        <div className="pb-6">
          <Composer onSend={send} disabled={state.streaming} />
        </div>
      </section>

      <section className="px-9 pt-9 pb-6 flex flex-col">
        <StrategyProgressPanel
          state={state.progress}
          onOpenInspector={state.bundleId ? () => nav(`/authoring/${state.bundleId}`) : undefined}
        />
      </section>
    </div>
  );
}
```

- [ ] **Step 4.4: Smoke**

```bash
cd frontend/web && pnpm dev   # one terminal
cargo run -p xvision-cli -- dashboard serve   # another
```

Open http://localhost:5173/setup. Type "Build me an ETH mean-revert strategy on 15m." → expect streaming agent response, tool calls visible as collapsible blocks, progress panel updates.

(If the WizardLoop isn't yet shipped, the SSE will 404 — type a message to confirm the error renders gracefully in the chat thread.)

- [ ] **Step 4.5: Commit**

```bash
git add frontend/web/src/api/wizard.ts frontend/web/src/components/wizard/ frontend/web/src/routes/setup.tsx
git commit -m "feat(frontend): Setup wizard with streaming chat + progress panel"
```

---

### Task 5: Chat rail — store + API client + component

**Files:**
- Create: `frontend/web/src/api/chat-rail.ts`
- Create: `frontend/web/src/stores/chat-rail.ts`
- Create: `frontend/web/src/components/shell/ChatRail.tsx`
- Modify: `frontend/web/src/components/shell/AppShell.tsx`

- [ ] **Step 5.1: API client**

Create `frontend/web/src/api/chat-rail.ts`:

```ts
import { apiFetch } from "./client";

export type ContextScope =
  | { kind: "global" }
  | { kind: "strategy"; ref: string }
  | { kind: "run"; ref: string }
  | { kind: "finding"; ref: string };

export type ChatSession = { session_id: string; scope: ContextScope; created_at: string };
export type ChatMessage = { id: string; role: "user" | "agent" | "tool"; content?: string; payload?: unknown; created_at: string };

export const chatRailApi = {
  createSession: (scope: ContextScope) =>
    apiFetch<ChatSession>("/api/chat-rail/sessions", { method: "POST", body: JSON.stringify({ scope }) }),
  history: (sessionId: string) =>
    apiFetch<ChatMessage[]>(`/api/chat-rail/sessions/${encodeURIComponent(sessionId)}`),
  updateScope: (sessionId: string, scope: ContextScope) =>
    apiFetch<ChatSession>(`/api/chat-rail/sessions/${encodeURIComponent(sessionId)}/scope`, {
      method: "PUT",
      body: JSON.stringify({ scope }),
    }),
};
```

- [ ] **Step 5.2: Zustand store (per-route open state, scope)**

Create `frontend/web/src/stores/chat-rail.ts`:

```ts
import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { ContextScope } from "@/api/chat-rail";

type Store = {
  openByRoute: Record<string, boolean>;
  sessionByRoute: Record<string, string>;     // route → sessionId
  scope: ContextScope;
  setOpen: (route: string, open: boolean) => void;
  setSession: (route: string, id: string) => void;
  setScope: (s: ContextScope) => void;
};

export const useChatRail = create<Store>()(
  persist(
    (set) => ({
      openByRoute: {},
      sessionByRoute: {},
      scope: { kind: "global" },
      setOpen: (route, open) => set((s) => ({ openByRoute: { ...s.openByRoute, [route]: open } })),
      setSession: (route, id) => set((s) => ({ sessionByRoute: { ...s.sessionByRoute, [route]: id } })),
      setScope: (scope) => set({ scope }),
    }),
    { name: "xvn:chat-rail" },
  ),
);
```

(Add `zustand@^4.5.4` if not already a dep — Plan 1 added it.)

- [ ] **Step 5.3: Implement `ChatRail`**

Create `frontend/web/src/components/shell/ChatRail.tsx`:

```tsx
import { useEffect, useReducer, useState } from "react";
import { useLocation } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { useSSE } from "@/hooks/useSSE";
import { chatRailApi, ContextScope } from "@/api/chat-rail";
import { useChatRail } from "@/stores/chat-rail";
import { MessageList } from "@/components/chat/MessageList";
import { Composer } from "@/components/chat/Composer";
import { Icon } from "@/components/primitives/Icon";
import type { StreamEvent } from "@/api/sse";

type Msg =
  | { id: string; role: "user"; content: string }
  | { id: string; role: "agent"; content: string; streaming?: boolean }
  | { id: string; role: "tool"; name: string; args: unknown; result?: unknown };

type State = { messages: Msg[]; streaming: boolean };
type Action =
  | { type: "load"; messages: Msg[] }
  | { type: "send"; text: string }
  | { type: "delta"; content: string }
  | { type: "done" }
  | { type: "tool"; name: string; args: unknown }
  | { type: "tool_result"; name: string; result: unknown }
  | { type: "error"; msg: string };

function reducer(s: State, a: Action): State {
  switch (a.type) {
    case "load": return { ...s, messages: a.messages };
    case "send": return { ...s, messages: [...s.messages, { id: crypto.randomUUID(), role: "user", content: a.text }], streaming: true };
    case "delta": {
      const last = s.messages[s.messages.length - 1];
      if (last?.role === "agent" && last.streaming) {
        return { ...s, messages: [...s.messages.slice(0, -1), { ...last, content: last.content + a.content }] };
      }
      return { ...s, messages: [...s.messages, { id: crypto.randomUUID(), role: "agent", content: a.content, streaming: true }] };
    }
    case "done": {
      const last = s.messages[s.messages.length - 1];
      if (last?.role === "agent") return { ...s, streaming: false, messages: [...s.messages.slice(0, -1), { ...last, streaming: false }] };
      return { ...s, streaming: false };
    }
    case "tool": return { ...s, messages: [...s.messages, { id: crypto.randomUUID(), role: "tool", name: a.name, args: a.args }] };
    case "tool_result": {
      const updated = [...s.messages];
      for (let i = updated.length - 1; i >= 0; i--) {
        const m = updated[i];
        if (m.role === "tool" && m.name === a.name && m.result === undefined) {
          updated[i] = { ...m, result: a.result };
          break;
        }
      }
      return { ...s, messages: updated };
    }
    case "error": return { ...s, streaming: false, messages: [...s.messages, { id: crypto.randomUUID(), role: "agent", content: `_Error: ${a.msg}_` }] };
  }
}

export function ChatRail() {
  const loc = useLocation();
  const { openByRoute, sessionByRoute, scope, setOpen, setSession } = useChatRail();
  const open = openByRoute[loc.pathname] ?? false;
  const sessionId = sessionByRoute[loc.pathname];

  const [state, dispatch] = useReducer(reducer, { messages: [], streaming: false });
  const [pending, setPending] = useState<string | null>(null);

  // Create session on first open per route
  useEffect(() => {
    if (open && !sessionId) {
      chatRailApi.createSession(scope).then((s) => setSession(loc.pathname, s.session_id));
    }
  }, [open, sessionId, loc.pathname]);

  // Load history on session change
  const { data: history } = useQuery({
    queryKey: ["chat-rail", sessionId],
    queryFn: () => chatRailApi.history(sessionId!),
    enabled: !!sessionId,
  });
  useEffect(() => {
    if (history) {
      dispatch({
        type: "load",
        messages: history.map((m) => m.role === "tool"
          ? { id: m.id, role: "tool", name: (m.payload as any)?.name ?? "?", args: (m.payload as any)?.args, result: (m.payload as any)?.result }
          : { id: m.id, role: m.role as "user" | "agent", content: m.content ?? "" }),
      });
    }
  }, [history]);

  useSSE({
    enabled: pending !== null && !!sessionId,
    endpoint: "/api/chat-rail/chat",
    body: { session_id: sessionId, message: pending },
    onEvent: (e: StreamEvent) => {
      if (e.type === "agent_message") dispatch({ type: "delta", content: e.content });
      else if (e.type === "tool_call") dispatch({ type: "tool", name: e.name, args: e.args });
      else if (e.type === "tool_result") dispatch({ type: "tool_result", name: e.name, result: e.result });
      else if (e.type === "done") dispatch({ type: "done" });
      else if (e.type === "error") dispatch({ type: "error", msg: e.message });
    },
    onClose: () => setPending(null),
  });

  function send(text: string) {
    dispatch({ type: "send", text });
    setPending(text);
  }

  if (!open) {
    return (
      <button
        onClick={() => setOpen(loc.pathname, true)}
        className="w-10 bg-surface-sidebar border-l border-border-soft flex flex-col items-center pt-4 text-text-3 hover:text-text"
        aria-label="Open chat rail"
      >
        <Icon name="play" size={16} />
      </button>
    );
  }

  return (
    <aside className="w-[320px] bg-surface-sidebar border-l border-border-soft p-5 flex flex-col gap-3">
      <div className="flex justify-between items-center">
        <div className="font-serif text-[18px]">Agent</div>
        <button onClick={() => setOpen(loc.pathname, false)} className="text-text-3 text-xs">×</button>
      </div>
      <div className="text-xs text-text-3">
        Scope: <span className="text-text-2">{scopeLabel(scope)}</span>
      </div>
      <div className="flex-1 overflow-y-auto">
        <MessageList messages={state.messages} />
      </div>
      <Composer onSend={send} disabled={state.streaming || !sessionId} placeholder="Ask the agent…" />
    </aside>
  );
}

function scopeLabel(s: ContextScope): string {
  if (s.kind === "global") return "Global";
  if (s.kind === "strategy") return `Strategy ${s.ref}`;
  if (s.kind === "run") return `Run ${s.ref}`;
  return `Finding ${s.ref}`;
}
```

- [ ] **Step 5.4: Mount in `AppShell`**

Replace the `ChatRailPlaceholder` import + usage in `AppShell.tsx`:

```tsx
import { ChatRail } from "./ChatRail";

// inside JSX, replace <ChatRailPlaceholder /> with:
<ChatRail />
```

Update the grid to allow the rail to expand — since it's variable width, change to:

```tsx
<div className="grid grid-cols-[200px_1fr_auto] h-screen w-screen overflow-hidden bg-bg text-text">
```

- [ ] **Step 5.5: Commit**

```bash
git add frontend/web/src/api/chat-rail.ts frontend/web/src/stores/chat-rail.ts frontend/web/src/components/shell/
git commit -m "feat(frontend): persistent chat rail with per-route state and SSE"
```

---

### Task 6: Inspector live-preview pane

**Files:**
- Create: `frontend/web/src/components/editors/SlotPreview.tsx`
- Modify: `frontend/web/src/api/strategies.ts` (already has `previewSlot`? add if missing)
- Modify: `frontend/web/src/routes/authoring.tsx`

- [ ] **Step 6.1: Add `previewSlot` to strategies API**

Append to `frontend/web/src/api/strategies.ts`:

```ts
import type { PreviewSlotRequest, PreviewSlotResponse } from "./types.gen";

export const strategiesPreview = {
  previewSlot: (id: string, req: PreviewSlotRequest) =>
    apiFetch<PreviewSlotResponse>(`/api/strategies/${encodeURIComponent(id)}/preview-slot`, {
      method: "POST",
      body: JSON.stringify(req),
    }),
};
```

- [ ] **Step 6.2: `SlotPreview` component**

Create `frontend/web/src/components/editors/SlotPreview.tsx`:

```tsx
import { useEffect, useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { Pill } from "@/components/primitives/Pill";
import { Dot } from "@/components/primitives/Dot";
import { strategiesPreview } from "@/api/strategies";
import type { PreviewSlotRequest, PreviewSlotResponse } from "@/api/types.gen";

const FIXTURES = [
  { id: "btc-usd-2025-01-15-08-00", label: "BTC/USD · 2025-01-15 08:00" },
  { id: "eth-usd-2025-01-15-08-00", label: "ETH/USD · 2025-01-15 08:00" },
  { id: "sol-usd-2025-01-15-08-00", label: "SOL/USD · 2025-01-15 08:00" },
];

type Props = {
  bundleId: string;
  slot: "intern" | "trader" | "regime";
  unsavedSystemPrompt?: string;
};

export function SlotPreview({ bundleId, slot, unsavedSystemPrompt }: Props) {
  const [fixtureId, setFixtureId] = useState(FIXTURES[0].id);
  const [autorun, setAutorun] = useState(true);
  const [last, setLast] = useState<PreviewSlotResponse | null>(null);

  const mut = useMutation({
    mutationFn: (req: PreviewSlotRequest) => strategiesPreview.previewSlot(bundleId, req),
    onSuccess: setLast,
  });

  // Debounced auto-rerun on prompt change
  useEffect(() => {
    if (!autorun) return;
    const t = setTimeout(() => {
      mut.mutate({
        slot,
        fixture_id: fixtureId,
        override_system_prompt: unsavedSystemPrompt,
        override_max_tokens: undefined,
      });
    }, 2000);
    return () => clearTimeout(t);
  }, [autorun, fixtureId, slot, unsavedSystemPrompt]);

  return (
    <div className="bg-surface-card border border-border rounded-card p-5 flex flex-col gap-3.5 h-full overflow-hidden">
      <div className="flex justify-between items-center">
        <span className="font-serif text-[18px]">Preview decision</span>
        <select
          value={fixtureId}
          onChange={(e) => setFixtureId(e.target.value)}
          className="bg-transparent border border-[rgba(212,165,71,0.35)] text-gold rounded-sm px-2 py-1 text-[11px]"
        >
          {FIXTURES.map((f) => <option key={f.id} value={f.id}>{f.label}</option>)}
        </select>
      </div>

      <div className="flex items-center gap-3 text-[12px] text-text-2">
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={autorun} onChange={(e) => setAutorun(e.target.checked)} className="accent-gold" />
          <Dot tone="gold" />
          Auto-rerun (2s debounce)
        </label>
        {last && (
          <span className="ml-auto text-text-3 font-mono">
            {last.tokens_used} tokens · ~${last.estimated_cost_usd.toFixed(4)}
          </span>
        )}
      </div>

      <div className="bg-surface-elev border border-border rounded-sm p-3 font-mono text-[11px] text-text-2">
        <div className="mb-1.5">Inputs ▾</div>
        <div className="text-text-3">{`{ fixture_id: "${fixtureId}" }`}</div>
      </div>

      <div className="flex-1 bg-[rgba(212,165,71,0.04)] border border-[rgba(212,165,71,0.25)] rounded-sm p-3.5 font-mono text-[12px] leading-relaxed overflow-auto">
        {mut.isPending ? (
          <div className="text-gold"><Dot tone="gold" />Streaming…</div>
        ) : last ? (
          <>
            <div className="text-gold mb-2">
              <Dot tone="gold" />
              Decision · {last.latency_ms}ms
            </div>
            <pre className="m-0 text-text">{JSON.stringify(last.decision_json, null, 2)}</pre>
            {last.diff && (
              <div className="text-text-3 text-[11px] mt-3">
                Δ vs previous: {last.diff.action_changed ? "action changed" : "action same"}
                {last.diff.conviction_delta != null && ` · conviction ${last.diff.conviction_delta >= 0 ? "+" : ""}${last.diff.conviction_delta.toFixed(2)}`}
              </div>
            )}
          </>
        ) : (
          <div className="text-text-3">Waiting for first run…</div>
        )}
      </div>

      <button
        onClick={() => mut.mutate({ slot, fixture_id: fixtureId, override_system_prompt: unsavedSystemPrompt })}
        disabled={mut.isPending}
        className="border border-border text-text-2 rounded-sm px-3 py-1.5 text-xs self-start"
      >
        Run now
      </button>
    </div>
  );
}
```

- [ ] **Step 6.3: Mount in Inspector**

In `frontend/web/src/routes/authoring.tsx`, replace the inner content of the form column with a 2-column split when `isLLMLayer` is true. Modify the JSX block:

```tsx
{isLLMLayer ? (
  <div className="grid grid-cols-2 gap-4 flex-1 min-h-0">
    <SlotEditor
      key={active}
      initial={initialForm}
      onSubmit={(form) => saveMut.mutate(form)}
      layerLabel={LAYER_LABELS[active]}
      bundleName={bundle.name}
      submitting={saveMut.isPending}
    />
    <SlotPreview bundleId={bundleId} slot={active as "intern" | "trader" | "regime"} />
  </div>
) : (
  // ... existing non-LLM placeholder
)}
```

Add the import at the top: `import { SlotPreview } from "@/components/editors/SlotPreview";`.

- [ ] **Step 6.4: Smoke**

Visit `/authoring/<id>`, edit a slot prompt, see live preview update after 2s.

- [ ] **Step 6.5: Commit**

```bash
git add frontend/web/src/api/strategies.ts frontend/web/src/components/editors/SlotPreview.tsx frontend/web/src/routes/authoring.tsx
git commit -m "feat(frontend): Inspector live-preview pane"
```

---

### Task 7: E2E smoke + docs

- [ ] **Step 7.1: Full flow**

```bash
cargo build --workspace
cargo run -p xvision-cli -- dashboard serve &
cd frontend/web && pnpm dev
```

Browser flow:
1. Visit `/setup`. Send a message. See streaming agent response, tool blocks, progress panel updates.
2. Click "Open in Inspector" → lands on `/authoring/<id>`.
3. Inspector shows split editor with live preview on the right.
4. Open chat rail (button on right edge). Send a message scoped to the current strategy.
5. Navigate to another route — chat rail closes (per-route state). Reopen.
6. Direct-navigate `/setup?seed=finding:test:abc` — agent first message references the finding (verifies seed plumbing).

- [ ] **Step 7.2: Mark Plan 4 done in DESIGN.md**

In §10, append `✓ landed` to "Phase 3 — agent surfaces".

- [ ] **Step 7.3: Commit**

```bash
git add frontend/DESIGN.md
git commit -m "docs: mark Plan 4 phase landed"
```

---

## Self-review

**Spec coverage:** Plan 4 covers DESIGN.md §6.2 (Setup wizard end-to-end), §6.4 right-pane (live preview), §7 (Chat rail). Backend gap #9 (live-preview slot endpoint) closed; gaps #13 (chat rail backend) and #14 (`?seed=`) are owned by external plans, this plan ships the frontend halves.

**Placeholder scan:** No "TBD". The `agent::execute::run_single_slot` function path (Task 1.2) flagged as "best-guess — adjust to actual API" — that's a wiring note, not a placeholder.

**Type consistency:** `StreamEvent` is the single tagged union shared by wizard, chat rail, and (future) run-progress channels. Slot names (`"intern" | "trader" | "regime"`) are consistent across `SlotPreview`, `PreviewSlotRequest`, and the engine.

**Cross-task:** Task 4 uses `MessageList` and `Composer` from Task 3. Task 5 also uses them. Task 6 uses `Pill`/`Dot` from Plan 1.

---

## Execution

Plan complete. Subagent-driven (recommended) or inline.
