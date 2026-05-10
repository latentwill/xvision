# Lab Notebook (`/journal`) Implementation Plan

> **Status: DEFERRED — POST-V1.** Not in the hackathon ship scope. Captured here so that when the post-hackathon roadmap picks it up, the work is fully specced and agentically executable. Do **not** start this work as part of v1.

> **For agentic workers (when this is picked up):** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Depends on:** Plan 2d (dashboard scaffold). Plan 3 (eval engine — provides finding-extracted events). Plan 2c (scheduler — provides deployment events). The Settings & Onboarding plan (because the journal page extends `base.html` after the chat rail and top-nav land).
> **Sequencing:** Lands after the eval engine and live cockpit have shipped — without those two surfaces the journal has no auto-pinned entries to display.

**Goal:** Ship `/journal` — the append-only chronological surface for findings, notes, postmortems, wizard recaps, deployment events, and eval milestones. The journal is the user's research substrate that outlasts any individual strategy.

**Architecture:** A new SQLite-backed table `journal_entries` lives in `~/.xvn/xvn.db`. Journal writes happen from three places: (a) explicit user composer input from `/journal`, (b) `Add to journal` button on findings panels, (c) auto-pinning hooks that subscribe to existing event streams (finding-extracted, deployment_event, eval_milestone, wizard_recap). The page is a single chronological column (max 880px wide) with a right inspector rail for filters + summary stats. The chat rail (Plan: chat-rail-persistence) docks outside the inspector rail.

**Tech Stack:** Rust 2021. New crate dep: `rusqlite = { version = "0.31", features = ["bundled"] }` for the journal table. Reuses Plan 2d's axum + askama + plain JS stack.

**Out of scope:**
- Cross-workspace journal (single XVN_HOME only).
- Journal export beyond JSON / Markdown dump.
- Inline collaborative editing.
- The `/lab` Power Notebook archetype (deferred-archetypes-roadmap.md tracks that as a separate post-v1 surface).

---

## File structure

```
crates/
├── xianvec-engine/
│   └── src/
│       ├── journal/
│       │   ├── mod.rs                          # NEW: pub use; module docs
│       │   ├── store.rs                        # NEW: rusqlite-backed JournalStore
│       │   ├── entry.rs                        # NEW: JournalEntry + JournalKind enums
│       │   └── auto_pin.rs                     # NEW: hooks that subscribe to event streams
│       └── lib.rs                              # MODIFY: pub mod journal;
├── xianvec-dashboard/
│   ├── src/routes/
│   │   └── journal.rs                          # NEW: GET /journal, REST handlers, SSE for new entries
│   ├── templates/
│   │   ├── journal.html                        # NEW
│   │   └── base.html                           # MODIFY: add Journal nav link (already in v0.2)
│   └── static/js/
│       └── journal.js                          # NEW: composer + filter + entry rendering
└── xianvec-cli/
    └── src/commands/
        └── journal.rs                          # NEW: `xvn journal {list, add, export}` for terminal users
```

---

## Phase A — Storage + entry model

### Task 1: `JournalEntry` + `JournalKind` enum

**Files:**
- Create: `crates/xianvec-engine/src/journal/entry.rs`
- Create: `crates/xianvec-engine/src/journal/mod.rs`
- Modify: `crates/xianvec-engine/src/lib.rs`

Per ui-elements.md §9.2 — six kinds.

- [ ] **Step 1: Define types**

```rust
//! Journal entries — the research substrate.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JournalKind {
    Finding,
    Note,
    Postmortem,
    WizardRecap,
    DeploymentEvent,
    EvalMilestone,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity { Info, Warning, Critical }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub id: String,                          // ULID, monospace-displayable
    pub kind: JournalKind,
    pub ts: DateTime<Utc>,
    pub author: Author,
    pub body_markdown: String,
    pub severity: Option<FindingSeverity>,   // populated for Finding kind
    pub refs: EntryRefs,
    pub tags: Vec<String>,                   // user-added: "#chop", "#funding"
    pub pinned: bool,                        // ★ flag
    pub unread: bool,                        // false once user expands the entry
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Author { User, System, WizardAgent }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntryRefs {
    pub run_id: Option<String>,
    pub agent_id: Option<String>,
    pub finding_id: Option<String>,
    pub scenario_id: Option<String>,
    pub deployment_id: Option<String>,
    pub linked_entry_id: Option<String>,     // for "Add note to this entry"
}

impl JournalEntry {
    pub fn new_note(body: String, refs: EntryRefs) -> Self {
        Self {
            id: Ulid::new().to_string(),
            kind: JournalKind::Note,
            ts: Utc::now(),
            author: Author::User,
            body_markdown: body,
            severity: None,
            refs,
            tags: vec![],
            pinned: false,
            unread: false,
        }
    }
}
```

- [ ] **Step 2: Module wiring**

```rust
// journal/mod.rs
pub mod entry;
pub mod store;
pub mod auto_pin;

pub use entry::*;
pub use store::JournalStore;
```

- [ ] **Step 3: Tests for serde round-trip + new_note defaults**

```rust
#[test]
fn note_round_trips_through_json() {
    let entry = JournalEntry::new_note("learned something".into(), EntryRefs::default());
    let s = serde_json::to_string(&entry).unwrap();
    let back: JournalEntry = serde_json::from_str(&s).unwrap();
    assert_eq!(back.body_markdown, "learned something");
    assert_eq!(back.kind, JournalKind::Note);
}
```

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(engine): journal entry types + serde wiring"
```

---

### Task 2: `JournalStore` (rusqlite-backed)

**Files:**
- Create: `crates/xianvec-engine/src/journal/store.rs`
- Modify: `crates/xianvec-engine/Cargo.toml` (add rusqlite)

Schema:

```sql
CREATE TABLE IF NOT EXISTS journal_entries (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    ts TEXT NOT NULL,                    -- ISO 8601
    author TEXT NOT NULL,
    body_markdown TEXT NOT NULL,
    severity TEXT,                       -- nullable
    refs_json TEXT NOT NULL DEFAULT '{}',
    tags_json TEXT NOT NULL DEFAULT '[]',
    pinned INTEGER NOT NULL DEFAULT 0,
    unread INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_journal_ts ON journal_entries(ts DESC);
CREATE INDEX IF NOT EXISTS idx_journal_kind ON journal_entries(kind);
CREATE INDEX IF NOT EXISTS idx_journal_pinned ON journal_entries(pinned) WHERE pinned = 1;
```

- [ ] **Step 1: Open-or-create + migrate**

```rust
pub struct JournalStore { conn: rusqlite::Connection }

impl JournalStore {
    pub fn open(xvn_home: &std::path::Path) -> anyhow::Result<Self> {
        let path = xvn_home.join("xvn.db");
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }
    /* ...insert / list / filter / pin / mark_read / delete... */
}

const SCHEMA: &str = r#"<paste schema above>"#;
```

- [ ] **Step 2: Insert + list_recent**

```rust
impl JournalStore {
    pub fn insert(&self, entry: &JournalEntry) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO journal_entries
              (id, kind, ts, author, body_markdown, severity, refs_json, tags_json, pinned, unread)
              VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                entry.id,
                serde_json::to_string(&entry.kind).unwrap().trim_matches('"'),
                entry.ts.to_rfc3339(),
                serde_json::to_string(&entry.author).unwrap().trim_matches('"'),
                entry.body_markdown,
                entry.severity.as_ref().map(|s| serde_json::to_string(s).unwrap().trim_matches('"').to_string()),
                serde_json::to_string(&entry.refs)?,
                serde_json::to_string(&entry.tags)?,
                entry.pinned as i64,
                entry.unread as i64,
            ],
        )?;
        Ok(())
    }

    pub fn list_recent(&self, limit: usize, filter: &Filter) -> anyhow::Result<Vec<JournalEntry>> {
        // SELECT * ORDER BY ts DESC LIMIT ?, with WHERE clauses for kinds/severity/tags
        // Return Vec<JournalEntry> via row mapping
        todo!("see test below for required behavior")
    }
}

#[derive(Default, Debug)]
pub struct Filter {
    pub kinds: Vec<JournalKind>,
    pub severities: Vec<FindingSeverity>,
    pub agent_ids: Vec<String>,
    pub tags: Vec<String>,
    pub from_ts: Option<DateTime<Utc>>,
    pub to_ts: Option<DateTime<Utc>>,
    pub pinned_only: bool,
}
```

- [ ] **Step 3: Pin / unpin / mark_read / delete + tests**

```rust
#[test]
fn insert_and_list_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let store = JournalStore::open(dir.path()).unwrap();
    let entry = JournalEntry::new_note("hi".into(), EntryRefs::default());
    store.insert(&entry).unwrap();
    let rows = store.list_recent(10, &Filter::default()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].body_markdown, "hi");
}

#[test]
fn filter_by_kind_works() { /* ... */ }
#[test]
fn pin_marks_entry_pinned() { /* ... */ }
```

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(engine): JournalStore with rusqlite + filter/pin/mark-read"
```

---

### Task 3: Auto-pin hooks

**File:** `crates/xianvec-engine/src/journal/auto_pin.rs`

Per ui-elements.md §9.2 auto-pinning rules:
- All `critical` findings auto-pin.
- `warning` findings only when user clicks `Add to journal`.
- `info` findings never auto-pin.
- All `deployment_event` entries auto-pin.
- `eval_milestone` thresholds: 5/10/25/50 evals on one strategy; first passing eval on a new scenario; new best Sharpe.

- [ ] **Step 1: Hook trait + concrete subscribers**

```rust
//! Auto-pin hooks subscribe to existing event streams and write to the journal.
use crate::journal::{JournalEntry, JournalKind, JournalStore, FindingSeverity, Author, EntryRefs};

pub struct AutoPinService { store: std::sync::Arc<JournalStore> }

impl AutoPinService {
    pub fn new(store: std::sync::Arc<JournalStore>) -> Self { Self { store } }

    /// Called by the eval engine when a finding is extracted.
    pub fn on_finding_extracted(&self, finding: &Finding, run_id: &str, agent_id: &str) -> anyhow::Result<()> {
        if finding.severity != FindingSeverity::Critical {
            return Ok(());   // warning + info do not auto-pin (user opt-in via UI)
        }
        let entry = JournalEntry {
            id: ulid::Ulid::new().to_string(),
            kind: JournalKind::Finding,
            ts: chrono::Utc::now(),
            author: Author::System,
            body_markdown: format!("**{}** — {}", finding.kind, finding.summary),
            severity: Some(finding.severity.clone()),
            refs: EntryRefs {
                run_id: Some(run_id.into()),
                agent_id: Some(agent_id.into()),
                finding_id: Some(finding.id.clone()),
                ..Default::default()
            },
            tags: vec![],
            pinned: true,        // critical findings start pinned
            unread: true,
        };
        self.store.insert(&entry)
    }

    pub fn on_deployment_event(&self, event: &DeploymentEvent) -> anyhow::Result<()> {
        let entry = JournalEntry {
            id: ulid::Ulid::new().to_string(),
            kind: JournalKind::DeploymentEvent,
            ts: chrono::Utc::now(),
            author: Author::System,
            body_markdown: event.render_markdown(),    // "paper-eth-mr started" etc
            severity: None,
            refs: EntryRefs { deployment_id: Some(event.deployment_id.clone()), ..Default::default() },
            tags: vec![],
            pinned: false,
            unread: true,
        };
        self.store.insert(&entry)
    }

    pub fn on_eval_completed(&self, run: &RunSummary) -> anyhow::Result<()> {
        let trigger = self.eval_milestone_trigger(run);
        if trigger.is_none() { return Ok(()); }
        // build entry with body_markdown derived from trigger; insert
        Ok(())
    }

    fn eval_milestone_trigger(&self, run: &RunSummary) -> Option<MilestoneKind> {
        // count prior eval_completed entries for this agent_id; if total ∈ {5, 10, 25, 50} → Threshold
        // check if scenario_id is new for this strategy → FirstPassingOnNewScenario
        // compare run.metrics.sharpe to historical max for this strategy → NewBestSharpe
        todo!()
    }
}

enum MilestoneKind { Threshold(u32), FirstPassingOnNewScenario, NewBestSharpe(f64) }
```

- [ ] **Step 2: Wizard recap hook** (called by Plan 2d's WizardLoop after each successful tool round)

```rust
pub fn on_wizard_action(&self, draft_id: &str, summary: &str) -> anyhow::Result<()> {
    // body_markdown e.g. "Drafted eth-mr-v3 from finding 01H8..." with link to draft
}
```

- [ ] **Step 3: Tests**

```rust
#[test]
fn critical_finding_auto_pins() {
    let dir = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(JournalStore::open(dir.path()).unwrap());
    let svc = AutoPinService::new(store.clone());
    let finding = Finding { id: "f1".into(), kind: "regime_fit_mismatch".into(),
        summary: "...".into(), severity: FindingSeverity::Critical };
    svc.on_finding_extracted(&finding, "run-001", "eth-mr-v3").unwrap();
    let rows = store.list_recent(10, &Filter::default()).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].pinned);
}

#[test]
fn warning_finding_does_not_auto_pin() { /* ... */ }
#[test]
fn eval_milestone_fires_at_5_evals() { /* ... */ }
#[test]
fn new_best_sharpe_emits_milestone() { /* ... */ }
```

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(engine): auto-pin service for journal entries"
```

---

## Phase B — Dashboard route + composer

### Task 4: GET /journal — page + filtered list endpoint

**Files:**
- Create: `crates/xianvec-dashboard/src/routes/journal.rs`
- Create: `crates/xianvec-dashboard/templates/journal.html`

- [ ] **Step 1: Template (per ui-elements.md §9 layout)**

Single chronological column max-w-[880px], right inspector rail with filters + pinned + last-30-days stats + tags. Sticky composer at top below header.

Skeleton:

```html
{% extends "base.html" %}
{% block main %}
<div class="grid grid-cols-[1fr_280px] gap-6">
  <section class="max-w-[880px]">
    <header class="flex items-center justify-between mb-4">
      <div>
        <h1 class="text-2xl">Journal</h1>
        <p class="text-secondary text-sm" id="journal-counts">…</p>
      </div>
      <div class="flex gap-2">
        <button class="btn-ghost" id="filter-btn">Filter…</button>
        <button class="btn-ghost" id="export-btn">Export</button>
        <button class="btn-primary" id="new-note-btn">+ New note</button>
      </div>
    </header>

    {# Composer (sticky top below header) — see §9.3 #}
    <div class="card mb-6 sticky top-2 z-10" id="composer-shell" hidden>
      <select id="entry-template" class="bg-panel border border-soft rounded px-2 py-1 mb-2">
        <option value="note">Note</option>
        <option value="postmortem">Postmortem</option>
        <option value="hypothesis">Hypothesis</option>
        <option value="decision_log">Decision log</option>
      </select>
      <textarea id="entry-body" rows="6" class="w-full bg-panel border border-soft rounded p-3 mono text-sm" placeholder="What did you learn?"></textarea>
      <div class="flex gap-2 justify-between items-center mt-2">
        <div id="entry-attachments" class="flex gap-2 flex-wrap">
          <button class="btn-ghost text-xs" data-attach="run">+ Run</button>
          <button class="btn-ghost text-xs" data-attach="strategy">+ Strategy</button>
          <button class="btn-ghost text-xs" data-attach="finding">+ Finding</button>
          <button class="btn-ghost text-xs" data-attach="scenario">+ Scenario</button>
        </div>
        <button class="btn-primary" id="entry-submit">Add to journal (⌘↵)</button>
      </div>
    </div>

    <div id="entries"></div>
  </section>

  <aside class="card h-fit sticky top-6">
    <div class="text-xs uppercase text-secondary mb-2">Filter</div>
    <div id="filter-panel">…kind toggles, severity multi-select, strategy multi-select, scenario multi-select, date range…</div>
    <hr class="my-3 border-soft">
    <div class="text-xs uppercase text-secondary mb-2">Pinned</div>
    <div id="pinned-list">…</div>
    <hr class="my-3 border-soft">
    <div class="text-xs uppercase text-secondary mb-2">Last 30 days</div>
    <div id="stats-30">findings · notes · drafts forked</div>
    <hr class="my-3 border-soft">
    <div class="text-xs uppercase text-secondary mb-2">Tags</div>
    <div id="tags-list" class="flex gap-1 flex-wrap">…</div>
  </aside>
</div>
{% endblock %}
{% block scripts %}<script type="module" src="/static/js/journal.js"></script>{% endblock %}
```

- [ ] **Step 2: Endpoints**

```rust
GET /journal                                     // page render
GET /api/journal?kinds=…&severities=…&...        // returns Vec<JournalEntry>
POST /api/journal                                // body: NewEntryReq → inserts, returns entry
PUT  /api/journal/:id/pin                        // toggles pinned
POST /api/journal/:id/read                       // marks unread=false
POST /api/journal/:id/tags                       // body: {tags: [...]} replaces
DELETE /api/journal/:id
GET  /api/journal/stream                         // SSE: pushes new entries as they're auto-pinned
```

- [ ] **Step 3: Tests**

```rust
#[tokio::test]
async fn list_returns_recent_entries() { /* seed 3 entries, filter on kind, expect 2 */ }
#[tokio::test]
async fn pin_toggles_state() { /* ... */ }
#[tokio::test]
async fn sse_stream_emits_new_entries() { /* ... */ }
```

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(dashboard): /journal page + REST + SSE stream"
```

---

### Task 5: Frontend `journal.js`

**File:** `crates/xianvec-dashboard/static/js/journal.js`

- [ ] **Step 1: Render entries**

For each entry, render a card with kind chip, ts, author chip, body, attachments (run/strategy/finding chips), per-row actions (`★ Pin`, `🔗 Copy link`, `📝 Add note to this entry`, `Draft variant from this →` (where applicable), `⋯ More`).

```javascript
async function refresh() {
  const filter = readFilterPanel();
  const url = '/api/journal?' + new URLSearchParams(filter).toString();
  const entries = await fetch(url).then(r => r.json());
  document.getElementById('entries').innerHTML = '';
  for (const e of entries) renderEntry(e);
  updateStats();
  updatePinnedList();
}

function renderEntry(e) {
  const card = document.createElement('div');
  card.className = 'card mb-3 ' + (e.unread ? 'border-l-4 border-l-mint' : '');
  card.innerHTML = `
    <header class="flex items-center gap-3 mb-2">
      <span class="pill kind-${e.kind}">${e.kind.replace('_', ' ')}</span>
      <span class="text-tertiary text-xs mono">${relativeTs(e.ts)}</span>
      <span class="text-tertiary text-xs">${e.author}</span>
      ${e.severity ? `<span class="pill severity-${e.severity}">${e.severity}</span>` : ''}
      ${e.pinned ? '<span class="ml-auto text-mint">★</span>' : ''}
    </header>
    <div class="markdown-body">${renderMarkdown(e.body_markdown)}</div>
    <div class="flex gap-2 mt-2 text-xs">
      ${e.refs.run_id ? `<a href="/eval/runs/${e.refs.run_id}" class="pill hover:text-mint">run ${shortId(e.refs.run_id)}</a>` : ''}
      ${e.refs.agent_id ? `<a href="/authoring/${e.refs.agent_id}" class="pill hover:text-mint">${e.refs.agent_id}</a>` : ''}
      ${e.refs.deployment_id ? `<a href="/live/${e.refs.deployment_id}" class="pill hover:text-mint">deployment</a>` : ''}
    </div>
    <footer class="mt-3 flex gap-2 text-xs text-secondary">
      <button data-act="pin" data-id="${e.id}">${e.pinned ? '★ Unpin' : '☆ Pin'}</button>
      <button data-act="copy" data-id="${e.id}">🔗 Link</button>
      <button data-act="add-note" data-id="${e.id}">📝 Add note</button>
      ${(e.kind === 'finding' || e.kind === 'eval_milestone') ? `<a class="hover:text-mint" href="/setup?seed=journal:${e.id}">Draft variant from this →</a>` : ''}
      <button data-act="more" data-id="${e.id}">⋯</button>
    </footer>`;
  document.getElementById('entries').appendChild(card);
  if (e.unread) markRead(e.id);
}
```

- [ ] **Step 2: Composer + submit**

```javascript
document.getElementById('new-note-btn').onclick = () => {
  document.getElementById('composer-shell').hidden = false;
  document.getElementById('entry-body').focus();
};

document.getElementById('entry-submit').onclick = async () => {
  const body = document.getElementById('entry-body').value.trim();
  if (!body) return;
  const template = document.getElementById('entry-template').value;
  const kind = template === 'note' ? 'note' : (template === 'postmortem' ? 'postmortem' : 'note');  // hypothesis + decision_log map to note kind in v1
  const refs = collectAttachmentRefs();
  await fetch('/api/journal', {
    method: 'POST',
    headers: {'content-type': 'application/json'},
    body: JSON.stringify({kind, body_markdown: applyTemplate(template, body), refs}),
  });
  document.getElementById('entry-body').value = '';
  document.getElementById('composer-shell').hidden = true;
  refresh();
};

function applyTemplate(t, body) {
  if (t === 'postmortem') return `## What I tried\n\n${body}\n\n## What worked\n\n## What didn't\n\n## Next\n`;
  if (t === 'hypothesis') return `**Hypothesis:** ${body}\n\n**To test:** \n\n**Expected:** `;
  if (t === 'decision_log') return `**Decision:** ${body}\n\n**Reasoning:** \n\n**Reversal cost:** `;
  return body;
}
```

- [ ] **Step 3: SSE subscriber for live auto-pin**

```javascript
const events = new EventSource('/api/journal/stream');
events.addEventListener('message', e => {
  const entry = JSON.parse(e.data);
  // prepend to entries list, show toast "auto-pinned: <kind>"
  showToast(`Pinned: ${entry.kind}`);
  refresh();
});
```

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(dashboard): journal.js with composer, templates, SSE auto-pin"
```

---

## Phase C — Auto-pin wiring + CLI parity

### Task 6: Wire eval engine + scheduler to AutoPinService

**Files:**
- Modify: `crates/xianvec-eval/src/finding_extractor.rs` (call `AutoPinService::on_finding_extracted`)
- Modify: `crates/xianvec-engine/src/scheduler.rs` (call `AutoPinService::on_deployment_event`)

The two emit-sites need a shared handle to `AutoPinService`. v1 wiring: a process-wide `Arc<AutoPinService>` registered on dashboard startup, accessed via a global `OnceCell` or by threading through existing daemon state. The latter is cleaner — thread it.

- [ ] **Step 1: Daemon state extension**

Add `AutoPinService` to whatever process-wide context the daemon already holds (`xianvec-engine::DaemonCtx` or equivalent). Eval and scheduler call sites pull it from `&ctx`.

- [ ] **Step 2: Tests** — exercise the integration with a fake event source.

- [ ] **Step 3: Commit**

```bash
git commit -am "feat(engine): wire eval + scheduler to AutoPinService for auto-pinning"
```

---

### Task 7: `xvn journal` CLI

**File:** `crates/xianvec-cli/src/commands/journal.rs`

```
xvn journal list [--kind <k>] [--limit N] [--pinned-only]
xvn journal add --kind note "<body>" [--run <id>] [--strategy <id>] ...
xvn journal export {json,md} > out.{json,md}
```

- [ ] Smoke + commit.

---

## Phase D — Polish

### Task 8: Empty state + first-finding moment

Per ui-elements.md §9.6: empty state copy `Your journal is empty. The first finding from your first eval will land here. You can also start with a hypothesis: New note → Hypothesis.` Render this when `list_recent` returns 0 rows.

### Task 9: Cross-route entry points

- `Add to journal` ghost button on Run detail findings panel (ui-elements.md §7.6) wires to `POST /api/journal` with severity from the finding.
- `Add to journal` ghost button on Compare findings panel (§8.5).
- `Recent findings` card on Control Tower (§2.3.1) shows the latest 5 journal entries with kind=finding.

### Task 10: Smoke + README

Manual journey:
1. Run an eval that produces a critical finding → entry auto-pins.
2. Open `/journal` → entry visible at top, pinned, unread.
3. Click `New note` → write a postmortem → submit → entry shows.
4. Pause a paper deployment → deployment_event entry auto-pins.
5. Filter by kind=finding → only finding rows.
6. Click `Draft variant from this →` on a finding → opens `/setup?seed=journal:<id>`.

Commit `chore: lab notebook smoke verified`.

---

## Self-review checklist

**Spec coverage:**
- [x] §9.1 Header strip — Task 4
- [x] §9.2 Six entry types + auto-pinning rules — Tasks 1–3
- [x] §9.3 Composer with templates — Task 5
- [x] §9.4 Right inspector rail (filters, pinned, stats, tags) — Task 4 + 5
- [x] §9.5 Per-entry actions — Task 5
- [x] §9.6 Empty state — Task 8
- [x] §9.7 Chat rail context — picked up by chat-rail-persistence plan, not this one

**Out of scope as planned:**
- [ ] `/lab` Power Notebook — separate (deferred archetype)
- [ ] Cross-workspace journal — single XVN_HOME only

**Type consistency:** `JournalEntry`, `JournalKind`, `EntryRefs`, `Filter`, all REST handlers.

**Frequent commits:** 10 tasks → ~10 commits.

---

## What's next

When v2 picks this up, the natural next thing is the Power Notebook (`/lab`) archetype — it's a deeper analytical surface for cell-based exploration, sitting above the journal. See `docs/superpowers/plans/2026-05-10-deferred-archetypes-roadmap.md`.

---

## Why this is deferred

- v1's reflection loop can run with the existing `findings` panel on `/eval/runs/<id>` plus `/eval/compare`. The journal is the sustained-research surface, not the per-run surface, and the per-run surface is enough to demo and ship.
- Auto-pinning has dependencies (eval engine + scheduler) that are themselves under active development; landing them first gives this plan stable event sources to subscribe to.
- The chat-rail context-switching design (`chat-rail-persistence` plan) is a sibling change to the journal's UI; pairing them post-v1 lets us think about them as one chrome update rather than two.
