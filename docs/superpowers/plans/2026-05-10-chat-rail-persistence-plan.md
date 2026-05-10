# Chat Rail Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Depends on:** Plan 2d (`xianvec-dashboard` scaffold + `WizardLoop` server-side agent loop). Settings & Onboarding plan (because the chat rail's empty state links to `/settings/providers`).
> **Sequencing:** Lands after Plan 2d ships the standalone `/setup` Wizard. This plan promotes that wizard from a single page into a persistent rail across every authenticated route — Move B from `docs/design/ui-elements.md` v0.2.

**Goal:** A right-side chat rail (Move B) on every authenticated route. Width 360px expanded, 40px icon-strip collapsed. The rail is open by default only on `/setup`; elsewhere it starts collapsed. Per-route open/closed state is remembered in localStorage. The wizard's conversation persists across route changes for the session, so the user can navigate freely without losing context.

**Architecture:** The standalone `/setup` Wizard's `WizardLoop` becomes the engine for the rail too — same server-side LLM agent, same MCP tool dispatch. New layer: `ChatSession` rows in `~/.xvn/xvn.db` carry the message history per session id. The rail is included via a single askama partial (`_chat_rail.html`) referenced from `base.html`. Frontend is a small JS module that handles the collapse toggle, mounts the chat thread, and binds route-specific quick replies + context chips. Cross-cycle context-handoff (e.g. clicking `Draft variant from this finding →`) opens `/setup?seed=<seed-id>` — the seed mechanism is owned by the **dashboard plan addition** task (live-preview + draft-variant subtasks, separate plan extension). This plan focuses on the rail itself.

**Tech Stack:** Rust 2021. Reuses `rusqlite` if Lab Notebook or Command Palette already pulled it in; otherwise this plan adds it. Reuses Plan 2d's axum + askama + plain JS.

**Out of scope:**
- Multi-user shared session state (single-user localhost only).
- Voice / transcription input.
- Wizard-initiated proactive nudges (the "I noticed your paper deploy dropped 3% — want to look?" feature). The unread dot UI exists but the producer that emits the nudge is deferred to a v1.1 follow-up. v1 supports manual chat only.
- Cross-device session sync.

---

## File structure

```
crates/
├── xianvec-engine/
│   └── src/
│       └── chat_session/
│           ├── mod.rs                          # NEW
│           ├── store.rs                        # NEW: rusqlite-backed session + message tables
│           └── context.rs                      # NEW: ContextScope enum + route → context mapping
├── xianvec-dashboard/
│   ├── src/
│   │   ├── routes/
│   │   │   ├── chat_rail.rs                    # NEW: /api/chat-rail/* endpoints
│   │   │   └── wizard.rs                       # MODIFY: factor LLM loop to share with rail
│   │   ├── wizard_loop.rs                      # MODIFY: accept ContextScope param
│   │   └── lib.rs                              # MODIFY: include chat_session arc in AppState
│   ├── templates/
│   │   ├── _chat_rail.html                     # NEW: partial included by base.html
│   │   └── base.html                           # MODIFY: include _chat_rail.html
│   └── static/js/
│       └── chat_rail.js                        # NEW: collapse + context chip + quick replies
```

---

## Phase A — Session storage + context

### Task 1: `chat_sessions` + `chat_messages` tables

**File:** `crates/xianvec-engine/src/chat_session/store.rs`

```sql
CREATE TABLE IF NOT EXISTS chat_sessions (
    id TEXT PRIMARY KEY,
    started_at TEXT NOT NULL,
    last_activity_at TEXT NOT NULL,
    context_scope_json TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS chat_messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    role TEXT NOT NULL,           -- "user" | "assistant"
    content_blocks_json TEXT NOT NULL,
    ts TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_chat_messages_session ON chat_messages(session_id, seq);
```

- [ ] **Step 1: `ChatSessionStore` open + insert + load**

```rust
pub struct ChatSessionStore { conn: rusqlite::Connection }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String, pub session_id: String, pub seq: i64,
    pub role: String, pub content_blocks: Vec<serde_json::Value>,
    pub ts: chrono::DateTime<chrono::Utc>,
}

impl ChatSessionStore {
    pub fn open(xvn_home: &Path) -> Result<Self> { /* same pattern as JournalStore */ }
    pub fn create_session(&self, scope: &ContextScope) -> Result<String> { /* returns ULID */ }
    pub fn append(&self, session_id: &str, role: &str, blocks: &[serde_json::Value]) -> Result<ChatMessage> { /* atomically computes next seq */ }
    pub fn load_history(&self, session_id: &str) -> Result<Vec<ChatMessage>> { /* ORDER BY seq */ }
    pub fn touch(&self, session_id: &str) -> Result<()> { /* update last_activity_at */ }
    pub fn delete_session(&self, session_id: &str) -> Result<()> { /* for "Start fresh" */ }
}
```

- [ ] **Step 2: Tests for round-trip + seq monotonicity**

```rust
#[test]
fn append_assigns_monotonic_seq() {
    let dir = tempfile::tempdir().unwrap();
    let store = ChatSessionStore::open(dir.path()).unwrap();
    let sid = store.create_session(&ContextScope::Workspace).unwrap();
    let m1 = store.append(&sid, "user", &[serde_json::json!({"type":"text","text":"hi"})]).unwrap();
    let m2 = store.append(&sid, "assistant", &[serde_json::json!({"type":"text","text":"hello"})]).unwrap();
    assert_eq!(m1.seq, 0);
    assert_eq!(m2.seq, 1);
}

#[test]
fn load_history_returns_in_seq_order() { /* ... */ }
#[test]
fn delete_session_cascades_messages() { /* ... */ }
```

- [ ] **Step 3: Commit**

```bash
git commit -am "feat(engine): chat_session store with rusqlite + monotonic seq"
```

---

### Task 2: `ContextScope` enum

**File:** `crates/xianvec-engine/src/chat_session/context.rs`

The "Change context ▾" dropdown (`ui-elements.md` §1.4) and the per-route auto-context need a shared type.

- [ ] **Step 1: Type**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum ContextScope {
    Workspace,                                  // "Whole workspace"
    Route { route: String },                    // "This page" — auto-set from URL
    Run { run_id: String },                     // /eval/runs/<id>
    Strategy { draft_id: String },              // /authoring/<id>
    Deployment { deployment_id: String },       // /live/<id>
    Compare { run_ids: Vec<String> },           // /eval/compare?ids=…
    JournalFilter { kinds: Vec<String> },       // /journal
    Selection { items: Vec<String> },           // user-selected via "Selected items"
    Seed { seed_id: String },                   // for /setup?seed=… cross-cycle entry
}

impl ContextScope {
    pub fn header_label(&self) -> String { /* "Context: this run", "Context: editing trader in btc-momentum", etc */ }
    pub fn quick_replies(&self) -> Vec<&'static str> { /* per-scope chip set */ }
    pub fn placeholder(&self) -> &'static str { /* composer placeholder per scope */ }
}
```

- [ ] **Step 2: Per-scope quick replies + placeholders**

Hardcode the per-route chip sets from `ui-elements.md`:

| Scope | Quick replies | Composer placeholder |
|---|---|---|
| Workspace | `What needs my attention?` `Pick a draft to work on` `Summarize this week` | `Ask anything about your workspace…` |
| `Run { run_id }` | `Why did it underperform?` `Compare to its baseline` `Suggest a variant to draft` | `Ask about this run…` |
| `Strategy { draft_id }` | `Improve this prompt` `Why is this slot expensive?` `Suggest a tool to add` `Diff vs template` | `Edit this slot…` |
| `Deployment { deployment_id }` | `Is this drift real?` `Should I pause it?` `Draft a variant from yesterday's vetoes` | `Ask about this deployment…` |
| `Compare { … }` | `What do the winners share?` `Why did the worst run underperform?` `Suggest a synthesis variant` | `Ask about this comparison…` |
| `JournalFilter { … }` | `Summarize what I've learned this week` `What's my most repeated mistake?` `Suggest a variant based on recent findings` | `Ask about your journal…` |
| `Route { route: "/strategies" }` | `Help me pick which to work on` `Which has the worst recent eval?` `Suggest a fork from the top-of-list` | `Filter or fork…` |
| `Route { route: "/eval/runs" }` | `Pick the most suspicious run` `Find runs that disagree on the same scenario` `Suggest a new scenario to test` | `Ask about this run list…` |

- [ ] **Step 3: Tests**

```rust
#[test]
fn run_scope_has_three_quick_replies() {
    let s = ContextScope::Run { run_id: "abc".into() };
    assert_eq!(s.quick_replies().len(), 3);
}
```

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(engine): ContextScope with per-scope replies + placeholders"
```

---

## Phase B — WizardLoop refactor for rail use

### Task 3: WizardLoop accepts `ContextScope` + persists to store

**File:** `crates/xianvec-dashboard/src/wizard_loop.rs` (modifies Plan 2d Task 6)

- [ ] **Step 1: Accept `Arc<ChatSessionStore>` and `session_id`**

The Plan 2d `WizardLoop::new` currently takes a `ChatRequest` with the user's message. It accumulates messages in-memory only. Refactor:

```rust
pub struct WizardLoop {
    xvn_home: PathBuf,
    dispatch: Box<dyn LlmDispatch>,
    store: Arc<ChatSessionStore>,
    session_id: String,
    scope: ContextScope,
    pending_events: Vec<WizardEvent>,
    is_done: bool,
}

impl WizardLoop {
    pub async fn new(
        xvn_home: PathBuf,
        store: Arc<ChatSessionStore>,
        session_id: String,
        scope: ContextScope,
        new_message: String,
        dispatch: Box<dyn LlmDispatch>,
    ) -> Result<Self> {
        // Append new user message to store first.
        store.append(&session_id, "user", &[serde_json::json!({"type":"text","text":new_message})])?;
        Ok(Self { /* ... */ })
    }

    pub async fn next_event(&mut self) -> Option<WizardEvent> {
        // Same loop as Plan 2d, but:
        // - Build LLM request from store.load_history() instead of in-memory Vec<Message>
        // - System prompt now includes a context line: "Current context: <scope.header_label()>"
        // - Each assistant response gets persisted via store.append(session_id, "assistant", blocks)
    }
}
```

- [ ] **Step 2: System-prompt context-injection helper**

```rust
fn context_aware_system_prompt(scope: &ContextScope) -> String {
    let base = include_str!("../prompts/wizard.md");
    format!("{base}\n\n## Current context\n{}\n", scope.header_label())
}
```

When scope is `Run`, also prepend the run's metrics summary (sharpe, return, drawdown) so the agent can reason about it without making a tool call. When scope is `Strategy`, prepend the draft summary. (Tool calls remain available for deeper info.)

- [ ] **Step 3: Tests with mock dispatch**

```rust
#[tokio::test]
async fn wizard_loop_persists_assistant_message() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(ChatSessionStore::open(dir.path()).unwrap());
    let sid = store.create_session(&ContextScope::Workspace).unwrap();
    let mock = MockDispatch::with_responses(vec![/* one text block "ok" */]);
    let mut loop_ctx = WizardLoop::new(
        dir.path().to_path_buf(), store.clone(), sid.clone(),
        ContextScope::Workspace, "hi".into(), Box::new(mock),
    ).await.unwrap();
    while loop_ctx.next_event().await.is_some() {}
    let history = store.load_history(&sid).unwrap();
    assert_eq!(history.len(), 2);     // user "hi" + assistant "ok"
    assert_eq!(history[1].role, "assistant");
}
```

- [ ] **Step 4: Backwards compat for Plan 2d's `/setup` page**

The standalone `/setup` page's chat hook (Plan 2d Task 5 `chat()` handler) creates a session if `session_id` not present, then delegates to the same WizardLoop. The `/setup` route's scope is always `ContextScope::Workspace` unless a `?seed=` param is present (handled by the dashboard-plan-extension task).

- [ ] **Step 5: Commit**

```bash
git commit -am "refactor(dashboard): WizardLoop persists chat history + accepts ContextScope"
```

---

## Phase C — REST + SSE for the rail

### Task 4: `/api/chat-rail/*` endpoints

**File:** `crates/xianvec-dashboard/src/routes/chat_rail.rs`

```
POST /api/chat-rail/sessions           -> creates session with given scope; returns {session_id}
GET  /api/chat-rail/sessions/:id       -> returns metadata + last_activity
GET  /api/chat-rail/sessions/:id/history -> returns Vec<ChatMessage>
POST /api/chat-rail/sessions/:id/scope -> updates context scope mid-session
DELETE /api/chat-rail/sessions/:id     -> "Start fresh"
POST /api/chat-rail/chat (SSE)         -> body: {session_id, message}; streams WizardEvent
```

- [ ] **Step 1: Create + history handlers**

```rust
#[derive(Deserialize)]
pub struct CreateSessionReq { pub scope: ContextScope }

pub async fn create_session(
    State(state): State<AppState>, Json(req): Json<CreateSessionReq>,
) -> Json<serde_json::Value> {
    let id = state.chat_session_store.create_session(&req.scope).unwrap();
    Json(serde_json::json!({"session_id": id}))
}

pub async fn history(
    State(state): State<AppState>, Path(id): Path<String>,
) -> Json<Vec<ChatMessage>> {
    Json(state.chat_session_store.load_history(&id).unwrap_or_default())
}
```

- [ ] **Step 2: SSE chat handler — reuses WizardLoop**

Same shape as Plan 2d Task 5, but uses session id from the request body and persists via `ChatSessionStore`. The wizard loop is now stateless across HTTP requests because all state is in the store.

- [ ] **Step 3: Scope update handler**

```rust
pub async fn update_scope(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(scope): Json<ContextScope>,
) -> StatusCode {
    state.chat_session_store.update_scope(&id, &scope).unwrap();
    StatusCode::NO_CONTENT
}
```

- [ ] **Step 4: Tests**

```rust
#[tokio::test]
async fn create_then_history_returns_empty() { /* ... */ }
#[tokio::test]
async fn chat_persists_round_trip() { /* POST chat → history grows by 2 */ }
#[tokio::test]
async fn delete_session_starts_fresh() { /* ... */ }
```

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(dashboard): /api/chat-rail/* endpoints for session-aware chat"
```

---

## Phase D — Frontend rail

### Task 5: `_chat_rail.html` partial + base.html include

**Files:**
- Create: `crates/xianvec-dashboard/templates/_chat_rail.html`
- Modify: `crates/xianvec-dashboard/templates/base.html`

- [ ] **Step 1: Markup**

Per ui-elements.md §1.4: 360px expanded, 40px icon-strip collapsed.

```html
{# templates/_chat_rail.html #}
<aside id="chat-rail" class="chat-rail collapsed" aria-label="xvn agent">
  <button id="rail-toggle" class="rail-toggle" title="Open chat rail (⇧⌘.)">💬</button>
  <div class="rail-body">
    <header class="rail-header">
      <span class="rail-title">xvn agent · <span class="text-mint">● Online</span></span>
      <span class="rail-context-chip mono" id="rail-context">Context: workspace</span>
      <button class="btn-ghost text-xs" id="rail-change-context">Change context ▾</button>
      <button class="btn-ghost text-xs" id="rail-collapse">×</button>
    </header>
    <div class="rail-thread" id="rail-thread"></div>
    <div class="rail-quick-replies" id="rail-quick-replies"></div>
    <form class="rail-composer" id="rail-composer">
      <textarea id="rail-input" rows="2" placeholder="Ask anything…"></textarea>
      <button type="submit" class="btn-primary">Send</button>
      <button type="button" class="btn-ghost" id="rail-stop" hidden>Stop</button>
    </form>
    <footer class="rail-footer mono text-xs text-tertiary">
      <span id="rail-tokens">Tokens this session: 0</span>
      <a href="/setup?from-rail=1" class="float-right hover:text-mint">Open in /setup ↗</a>
      <button class="btn-ghost text-xs ml-2" id="rail-fresh">Start fresh</button>
    </footer>
  </div>
</aside>

<dialog id="rail-context-modal" class="card">
  <h3>Change context</h3>
  <ul class="list-none p-0">
    <li><label><input type="radio" name="scope" value="workspace"> Whole workspace</label></li>
    <li><label><input type="radio" name="scope" value="route"> This page</label></li>
    <li><label><input type="radio" name="scope" value="selection" disabled> Selected items <span class="text-tertiary">(soon)</span></label></li>
  </ul>
  <div class="flex gap-2 justify-end mt-4">
    <button class="btn-ghost" onclick="document.getElementById('rail-context-modal').close()">Cancel</button>
    <button class="btn-primary" id="rail-context-confirm">Apply</button>
  </div>
</dialog>
```

CSS (append to `theme.css`):

```css
.chat-rail {
  position: fixed; right: 0; top: 0; bottom: 0;
  display: grid; grid-template-columns: 40px 1fr;
  background: var(--bg-elevated); border-left: 1px solid var(--border);
  transition: width 0.2s ease;
  z-index: 40;
}
.chat-rail.collapsed { width: 40px; grid-template-columns: 40px; }
.chat-rail.expanded { width: 360px; }
.chat-rail.collapsed .rail-body { display: none; }
.rail-toggle { background: transparent; border: 0; color: var(--text-primary); width: 40px; cursor: pointer; }
.rail-body { display: flex; flex-direction: column; padding: 12px; }
.rail-header { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; padding-bottom: 8px; border-bottom: 1px solid var(--border); }
.rail-thread { flex: 1; overflow-y: auto; padding: 12px 0; display: flex; flex-direction: column; gap: 8px; }
.rail-quick-replies { display: flex; gap: 6px; flex-wrap: wrap; padding: 8px 0; }
.rail-quick-replies .pill { cursor: pointer; }
.rail-composer { display: grid; grid-template-columns: 1fr auto; gap: 6px; }
.rail-footer { padding-top: 8px; border-top: 1px solid var(--border); }
```

Adjust main content padding right to make room: in `base.html`'s body, wrap `<main>` so `padding-right: 40px;` (collapsed) or `360px` (expanded) adjusts dynamically. Alternative: since the rail is `position: fixed`, just set `body { padding-right: 40px; }` and toggle to 360 via a body-level class (`body.rail-expanded`) — keeps things simple.

- [ ] **Step 2: Include in base.html**

```html
{# templates/base.html — replace existing chat-rail-toggle button block #}
{% include "_chat_rail.html" %}
```

- [ ] **Step 3: Suppress rail on /setup**

The `/setup` route is the page-as-chat. The rail itself is hidden when route matches `/setup`. Add to `_chat_rail.html`:

```html
{% if route_path != "/setup" %}
{# rail markup #}
{% endif %}
```

`route_path` is plumbed into the askama template state from each handler.

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(dashboard): _chat_rail.html partial included from base"
```

---

### Task 6: `chat_rail.js` — collapse + per-route state + chat plumbing

**File:** `crates/xianvec-dashboard/static/js/chat_rail.js`

- [ ] **Step 1: Collapse toggle + per-route persistence**

```javascript
const rail = document.getElementById('chat-rail');
const toggleBtn = document.getElementById('rail-toggle');
const collapseBtn = document.getElementById('rail-collapse');
const route = window.location.pathname;

function railKey() { return `xvn_rail_open:${route}`; }
function applyState(open) {
  rail.classList.toggle('expanded', open);
  rail.classList.toggle('collapsed', !open);
  document.body.classList.toggle('rail-expanded', open);
  localStorage.setItem(railKey(), open ? '1' : '0');
}

// Default-open on /setup (handled server-side by suppressing rail entirely).
// Default-collapsed elsewhere; restore prior state if remembered.
const remembered = localStorage.getItem(railKey());
applyState(remembered === '1');

toggleBtn.onclick = () => applyState(!rail.classList.contains('expanded'));
collapseBtn.onclick = () => applyState(false);

// Keyboard shortcut: ⇧⌘. (Mac) / Shift+Ctrl+. (Linux/Win) toggles rail.
document.addEventListener('keydown', e => {
  const meta = navigator.platform.includes('Mac') ? e.metaKey : e.ctrlKey;
  if (meta && e.shiftKey && e.key === '.') {
    e.preventDefault();
    applyState(!rail.classList.contains('expanded'));
  }
});
```

- [ ] **Step 2: Session bootstrap**

```javascript
// Single session per browser tab, persisted in sessionStorage so it survives reloads.
let sessionId = sessionStorage.getItem('xvn_chat_session_id');
const scope = inferScopeFromRoute(route);

async function ensureSession() {
  if (sessionId) {
    const ok = await fetch(`/api/chat-rail/sessions/${sessionId}`).then(r => r.ok);
    if (ok) return;
  }
  const resp = await fetch('/api/chat-rail/sessions', {
    method: 'POST',
    headers: {'content-type': 'application/json'},
    body: JSON.stringify({scope}),
  });
  const json = await resp.json();
  sessionId = json.session_id;
  sessionStorage.setItem('xvn_chat_session_id', sessionId);
}

function inferScopeFromRoute(path) {
  if (path === '/' || path === '/setup') return {scope: 'workspace'};
  let m;
  if ((m = path.match(/^\/eval\/runs\/([^/?]+)$/))) return {scope: 'run', run_id: m[1]};
  if ((m = path.match(/^\/authoring\/([^/?]+)$/))) return {scope: 'strategy', draft_id: m[1]};
  if ((m = path.match(/^\/live\/([^/?]+)$/))) return {scope: 'deployment', deployment_id: m[1]};
  if (path.startsWith('/eval/compare')) {
    const ids = new URLSearchParams(window.location.search).get('ids')?.split(',') ?? [];
    return {scope: 'compare', run_ids: ids};
  }
  if (path.startsWith('/journal')) return {scope: 'journal_filter', kinds: []};
  return {scope: 'route', route: path};
}
```

- [ ] **Step 3: Render history + send + SSE handling**

```javascript
async function renderHistory() {
  const history = await fetch(`/api/chat-rail/sessions/${sessionId}/history`).then(r => r.json());
  const thread = document.getElementById('rail-thread');
  thread.innerHTML = '';
  for (const m of history) {
    for (const block of m.content_blocks) {
      if (block.type === 'text') appendBubble(m.role, block.text);
    }
  }
}

function appendBubble(role, text) { /* same shape as Plan 2d wizard.js */ }

document.getElementById('rail-composer').addEventListener('submit', async e => {
  e.preventDefault();
  const input = document.getElementById('rail-input');
  const msg = input.value.trim();
  if (!msg) return;
  input.value = '';
  appendBubble('user', msg);
  const resp = await fetch('/api/chat-rail/chat', {
    method: 'POST',
    headers: {'content-type': 'application/json'},
    body: JSON.stringify({session_id: sessionId, message: msg}),
  });
  await streamSseInto(resp, ev => /* update thread + tokens + context */ null);
});
```

- [ ] **Step 4: Quick replies + context chip**

```javascript
function renderQuickReplies(replies) {
  const root = document.getElementById('rail-quick-replies');
  root.innerHTML = '';
  for (const r of replies) {
    const chip = document.createElement('span');
    chip.className = 'pill';
    chip.textContent = r;
    chip.onclick = () => { document.getElementById('rail-input').value = r; };
    root.appendChild(chip);
  }
}

async function loadContextLabels() {
  const resp = await fetch(`/api/chat-rail/sessions/${sessionId}`).then(r => r.json());
  document.getElementById('rail-context').textContent = `Context: ${resp.scope_label}`;
  renderQuickReplies(resp.quick_replies);
  document.getElementById('rail-input').placeholder = resp.placeholder;
}
```

The session-detail endpoint returns `scope_label`, `quick_replies`, and `placeholder` derived server-side from `ContextScope::header_label()` / `quick_replies()` / `placeholder()`.

- [ ] **Step 5: "Start fresh" button**

```javascript
document.getElementById('rail-fresh').onclick = async () => {
  if (!confirm('Start a new conversation? Current chat history will be deleted.')) return;
  await fetch(`/api/chat-rail/sessions/${sessionId}`, {method: 'DELETE'});
  sessionStorage.removeItem('xvn_chat_session_id');
  sessionId = null;
  await ensureSession();
  document.getElementById('rail-thread').innerHTML = '';
};
```

- [ ] **Step 6: No-LLM-key empty state**

If `/api/llm-status` returns `{name: null}`, the rail body shows the same `Add an LLM key to begin` card as `/setup §3.3` with a `Set up keys →` link to `/settings/providers`. Hide the composer.

```javascript
async function checkKey() {
  const status = await fetch('/api/llm-status').then(r => r.json());
  if (!status.name) {
    document.getElementById('rail-thread').innerHTML = `
      <div class="card">
        <h4>Add an LLM key to begin</h4>
        <p class="text-secondary text-xs">xvn uses your key for both the setup agent and the strategies it builds.</p>
        <a class="btn-primary mt-2 inline-block" href="/settings/providers">Set up keys →</a>
      </div>`;
    document.getElementById('rail-composer').hidden = true;
    return false;
  }
  return true;
}
```

- [ ] **Step 7: Bootstrap on every page**

```javascript
(async () => {
  if (!(await checkKey())) return;
  await ensureSession();
  await renderHistory();
  await loadContextLabels();
})();
```

- [ ] **Step 8: Mount script in `base.html`**

```html
<script type="module" src="/static/js/chat_rail.js"></script>
```

- [ ] **Step 9: Commit**

```bash
git commit -am "feat(dashboard): chat_rail.js with collapse + per-route state + history + quick replies"
```

---

## Phase E — Smoke + polish

### Task 7: Manual end-to-end smoke

```bash
xvn  # opens dashboard
# 1. Land on / — rail should be collapsed (40px strip) by default.
# 2. ⇧⌘. → rail expands to 360px. Send "what's in my workspace?" → assistant streams.
# 3. Navigate to /eval/runs — rail still expanded (route-state remembered). Context chip reads "Context: route /eval/runs". Quick replies: 3 chips per ContextScope::Route.
# 4. Click a run → /eval/runs/<id>. Context chip flips to "Context: this run". Different quick replies appear.
# 5. Reload the page. History persists (session id in sessionStorage; messages in xvn.db).
# 6. Click "Start fresh" → confirm → thread empties; new session id.
# 7. With no key set (export -n ANTHROPIC_API_KEY), reload → rail body shows "Add an LLM key to begin".
```

Document in `crates/xianvec-dashboard/README.md` under "Chat Rail".

Commit `chore: chat rail persistence smoke verified`.

---

### Task 8: Wireframe-question resolution: §18.1 + §18.2

Per the open questions in `ui-elements.md` §18:

- **#1 First-run detection.** The Settings & Onboarding plan owns this — `/` redirects to `/setup` when no provider has a set key. This plan piggy-backs.
- **#2 Chat rail context-handoff continuity.** This plan's answer: session persists for the browser tab. When a `Draft variant from this →` button is clicked elsewhere, the user lands on `/setup`, where the rail is suppressed and the page-as-chat takes over. The dashboard plan's draft-variant-context-seeding subtask is responsible for seeding `/setup` with the new context. When the user navigates back, the rail uses the same `xvn_chat_session_id` it had before, so no message loss.

Cross-link these resolutions in a new doc note: `docs/superpowers/notes/2026-05-10-chat-rail-context-resolution.md` (one-pager that records the decision so the wireframer can lock it).

- [ ] Commit `docs: resolve chat-rail context handoff continuity per ui-elements.md §18.2`.

---

## Self-review checklist

**Spec coverage:**
- [x] §1.4 chat-permanent rail on every authenticated route — Tasks 5–6
- [x] §1.4 360px / 40px collapse states — Task 5 CSS, Task 6 JS
- [x] §1.4 Header strip with context chip + Change context ▾ — Tasks 5 + 6
- [x] §1.4 Composer with route-specific placeholder — Task 6 (loadContextLabels)
- [x] §1.4 Quick replies row, route-specific — Task 6 (renderQuickReplies)
- [x] §1.4 Footer with token count + Open in /setup ↗ + Start fresh — Tasks 5 + 6
- [x] §1.4 Collapse rules: open by default on /setup only — Task 5 (rail suppressed on /setup)
- [x] §1.4 Per-route open/closed state in localStorage — Task 6 (railKey)
- [x] §1.4 Empty state when no LLM key — Task 6 Step 6
- [x] §3.5 Cross-cycle entry points (Seed scope) — Task 2 enum has the variant; the actual seeding flow is owned by the dashboard plan extension
- [ ] §1.4 Unread-message dot for wizard nudges — Out of scope (v1.1, no producer yet)

**Out of scope as planned:**
- [ ] Wizard-initiated nudges
- [ ] Cross-device session sync
- [ ] Multi-user shared session

**Type consistency:** `ChatSessionStore`, `ContextScope`, `WizardLoop` (refactored) — consistent.

**Frequent commits:** 8 tasks → ~8 commits.

---

## What's next

After this lands, the rail is a primitive that future features can hang off:
- Wizard-initiated nudges become a small daemon-side service that periodically inserts assistant messages into open sessions when something interesting happens (3 chop failures, paper deploy drop, eval queue spike). The unread dot already exists.
- Lab Notebook plan picks up `JournalFilter` scope in its chat-rail integration.
- The dashboard-plan-extension `draft-variant-context-seeding` subtask resolves the `/setup?seed=…` handoff that makes Move I (`Draft variant from this →` buttons) work end-to-end with this rail.
