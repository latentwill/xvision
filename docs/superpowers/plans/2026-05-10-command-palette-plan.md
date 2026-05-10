# Command Palette (⌘K) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Depends on:** Plan 2d (dashboard scaffold + base.html). Settings & Onboarding plan (because the palette ships as part of v1 global chrome and assumes the rest of the auth-gated routes exist).
> **Optional dep:** Lab Notebook plan — if not yet shipped, the `Findings` group surfaces only the in-memory findings from the eval engine, not journal entries. Otherwise, journal-pinned findings are also indexed.

**Goal:** A keyboard-driven palette over every artifact in xvn — strategies, runs, findings, scenarios, deployments — plus a small set of named actions. Press `⌘K` (or `Ctrl+K` on Linux/Win), modal overlay opens, type-as-you-go fuzzy search across all artifact types.

**Architecture:** SQLite FTS5 virtual table inside `~/.xvn/xvn.db` (the same DB the journal uses). One row per artifact, kept in sync by lightweight indexers wired into the artifact create/update paths. Search is a single `SELECT … MATCH ?` with grouping by `kind` on the result side. The modal is a plain HTML `<dialog>` element rendered into `base.html`, styled with the design system tokens.

**Tech Stack:** Rust 2021. New crate dep: `rusqlite` features include `bundled` (already added if Lab Notebook landed first). FTS5 is part of bundled SQLite. No new frontend deps.

**Out of scope:**
- Personalized ranking (all results sort by FTS rank then `updated_at desc`).
- Full-text search of strategy prompts or finding bodies — v1 indexes name + summary + tags only. Body indexing is a v1.1 add when storage hits a few-MB scale.
- Global shortcut customization (the binding is hardcoded `⌘K` / `Ctrl+K` in v1).

---

## File structure

```
crates/
├── xvision-engine/
│   └── src/
│       └── search/
│           ├── mod.rs                          # NEW: re-exports
│           ├── index.rs                        # NEW: FTS5 schema + writer
│           ├── query.rs                        # NEW: search() + result types
│           └── indexers.rs                     # NEW: per-artifact indexer hooks
├── xvision-dashboard/
│   ├── src/routes/
│   │   └── search.rs                           # NEW: GET /api/search
│   ├── templates/
│   │   └── base.html                           # MODIFY: include palette modal
│   └── static/js/
│       └── command_palette.js                  # NEW: ⌘K binding + modal logic
```

---

## Phase A — Index + indexers

### Task 1: FTS5 schema + writer

**Files:**
- Create: `crates/xvision-engine/src/search/mod.rs`
- Create: `crates/xvision-engine/src/search/index.rs`

Schema:

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(
    artifact_id UNINDEXED,
    kind UNINDEXED,                  -- "strategy" | "run" | "finding" | "scenario" | "deployment" | "journal_entry"
    title,                            -- e.g. strategy name, run display name
    summary,                          -- short description
    tags,                             -- space-separated
    updated_at UNINDEXED,             -- ISO 8601
    href UNINDEXED,                   -- in-app URL for the result row
    tokenize='porter unicode61'
);
```

- [ ] **Step 1: Define `IndexEntry` struct + module skeleton**

```rust
// search/mod.rs
pub mod index;
pub mod query;
pub mod indexers;
pub use index::SearchIndex;
pub use query::{search, SearchHit, SearchKind};

// search/index.rs
use rusqlite::Connection;

pub struct SearchIndex { conn: rusqlite::Connection }

#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub artifact_id: String,
    pub kind: SearchKind,           // see query.rs
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub href: String,
}

impl SearchIndex {
    pub fn open(xvn_home: &std::path::Path) -> anyhow::Result<Self> {
        let path = xvn_home.join("xvn.db");
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn upsert(&self, entry: &IndexEntry) -> anyhow::Result<()> {
        // Delete-then-insert (FTS5 doesn't support UPSERT directly).
        self.conn.execute("DELETE FROM search_index WHERE artifact_id = ?1 AND kind = ?2",
            rusqlite::params![entry.artifact_id, entry.kind.as_str()])?;
        self.conn.execute(
            "INSERT INTO search_index (artifact_id, kind, title, summary, tags, updated_at, href)
              VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                entry.artifact_id, entry.kind.as_str(),
                entry.title, entry.summary,
                entry.tags.join(" "),
                entry.updated_at.to_rfc3339(),
                entry.href,
            ],
        )?;
        Ok(())
    }

    pub fn delete(&self, kind: SearchKind, artifact_id: &str) -> anyhow::Result<()> {
        self.conn.execute("DELETE FROM search_index WHERE artifact_id = ?1 AND kind = ?2",
            rusqlite::params![artifact_id, kind.as_str()])?;
        Ok(())
    }
}

const SCHEMA: &str = r#"<paste schema above>"#;
```

- [ ] **Step 2: Tests**

```rust
#[test]
fn upsert_and_search_by_title() {
    let dir = tempfile::tempdir().unwrap();
    let idx = SearchIndex::open(dir.path()).unwrap();
    idx.upsert(&IndexEntry {
        artifact_id: "btc-momentum".into(),
        kind: SearchKind::Strategy,
        title: "btc-momentum".into(),
        summary: "Trend follower on BTC perp".into(),
        tags: vec!["trend".into()],
        updated_at: chrono::Utc::now(),
        href: "/authoring/btc-momentum".into(),
    }).unwrap();
    let hits = crate::search::search(&idx, "btc", &Default::default()).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].artifact_id, "btc-momentum");
}

#[test]
fn upsert_deduplicates() { /* upsert same id twice → only one row */ }
```

- [ ] **Step 3: Commit**

```bash
git commit -am "feat(engine): SearchIndex with FTS5 backing"
```

---

### Task 2: `search()` query function

**File:** `crates/xvision-engine/src/search/query.rs`

- [ ] **Step 1: Types + query**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SearchKind { Strategy, Run, Finding, Scenario, Deployment, JournalEntry, Action }

impl SearchKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Strategy => "strategy", Self::Run => "run", Self::Finding => "finding",
            Self::Scenario => "scenario", Self::Deployment => "deployment",
            Self::JournalEntry => "journal_entry", Self::Action => "action",
        }
    }
}

#[derive(Debug, Default)]
pub struct SearchOpts { pub kinds: Option<Vec<SearchKind>>, pub limit: Option<usize> }

#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub artifact_id: String,
    pub kind: SearchKind,
    pub title: String,
    pub summary: String,
    pub href: String,
    pub rank: f64,
}

pub fn search(idx: &super::SearchIndex, q: &str, opts: &SearchOpts) -> anyhow::Result<Vec<SearchHit>> {
    if q.trim().is_empty() { return Ok(vec![]); }
    let limit = opts.limit.unwrap_or(40);
    // Wrap query for FTS5 — append `*` for prefix on trailing token, escape quotes.
    let fts_q = sanitize_fts(q);
    let kinds_clause = match &opts.kinds {
        Some(ks) if !ks.is_empty() => format!(" AND kind IN ({})", ks.iter().map(|k| format!("'{}'", k.as_str())).collect::<Vec<_>>().join(",")),
        _ => String::new(),
    };
    let sql = format!("SELECT artifact_id, kind, title, summary, href, rank FROM search_index WHERE search_index MATCH ?{kinds_clause} ORDER BY rank LIMIT ?");
    let mut stmt = idx.conn().prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params![fts_q, limit as i64], |row| {
        Ok(SearchHit {
            artifact_id: row.get(0)?, kind: parse_kind(&row.get::<_, String>(1)?),
            title: row.get(2)?, summary: row.get(3)?, href: row.get(4)?,
            rank: row.get::<_, f64>(5).unwrap_or(0.0),
        })
    })?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn sanitize_fts(q: &str) -> String {
    let cleaned: String = q.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '-').collect();
    let mut tokens: Vec<String> = cleaned.split_whitespace().map(|s| s.to_string()).collect();
    if let Some(last) = tokens.last_mut() { last.push('*'); }
    tokens.join(" ")
}

fn parse_kind(s: &str) -> SearchKind { /* match … */ }
```

- [ ] **Step 2: Tests**

```rust
#[test]
fn prefix_search_works() {
    /* index "btc-momentum" → query "btc-mom" finds it */
}

#[test]
fn kind_filter_excludes_other_kinds() { /* ... */ }

#[test]
fn empty_query_returns_zero_hits() { /* ... */ }
```

- [ ] **Step 3: Commit**

```bash
git commit -am "feat(engine): search() with prefix + kind filtering"
```

---

### Task 3: Per-artifact indexers

**File:** `crates/xvision-engine/src/search/indexers.rs`

Wire indexer calls into the existing artifact create/update paths. Each indexer is a small adapter that maps an artifact to an `IndexEntry`.

- [ ] **Step 1: Strategy indexer**

```rust
pub fn index_strategy(idx: &SearchIndex, draft: &StrategyDraft) -> anyhow::Result<()> {
    idx.upsert(&IndexEntry {
        artifact_id: draft.id.clone(),
        kind: SearchKind::Strategy,
        title: draft.name.clone(),
        summary: format!("{} · {}", draft.template_name, draft.summary.clone().unwrap_or_default()),
        tags: draft.tags.clone(),
        updated_at: draft.updated_at,
        href: format!("/authoring/{}", draft.id),
    })
}
```

Call this from `xvision-engine::mcp::authoring::create_strategy` and `update_slot`.

- [ ] **Step 2: Run indexer**

```rust
pub fn index_run(idx: &SearchIndex, run: &RunSummary) -> anyhow::Result<()> {
    idx.upsert(&IndexEntry {
        artifact_id: run.id.clone(),
        kind: SearchKind::Run,
        title: run.display_name.clone().unwrap_or_else(|| format!("Run {}", &run.id[..8])),
        summary: format!("{} · {} · sharpe {:.2}", run.strategy_name, run.scenario_name, run.metrics.sharpe),
        tags: vec![run.mode.to_string()],
        updated_at: run.started_at,
        href: format!("/eval/runs/{}", run.id),
    })
}
```

Call from `xvision-eval::run_completed` hook.

- [ ] **Step 3: Finding, Scenario, Deployment, JournalEntry indexers**

Each follows the same shape; wired into the corresponding create site. JournalEntry is wired in the Lab Notebook plan if/when shipped.

- [ ] **Step 4: Static action index**

Bootstrap-time, index a fixed list of named actions:

```rust
pub fn seed_actions(idx: &SearchIndex) -> anyhow::Result<()> {
    let actions = [
        ("new-strategy", "New strategy from template…", "Open the wizard with a template picker", "/setup?seed=template-picker"),
        ("new-run", "New eval run", "Pick strategy + scenario, run a backtest", "/eval/runs?new=1"),
        ("new-deploy", "Deploy a strategy", "Open the new-deployment modal", "/live?new=1"),
        ("settings-providers", "Add provider", "Add an LLM provider", "/settings/providers?add=1"),
        ("settings-broker", "Add broker", "Add a broker connection", "/settings/brokers?add=1"),
        ("export-journal", "Export journal", "Download journal as JSON", "/api/journal/export?fmt=json"),
    ];
    for (id, title, summary, href) in actions {
        idx.upsert(&IndexEntry {
            artifact_id: id.into(), kind: SearchKind::Action,
            title: title.into(), summary: summary.into(),
            tags: vec![], updated_at: chrono::Utc::now(),
            href: href.into(),
        })?;
    }
    Ok(())
}
```

- [ ] **Step 5: One-shot reindex on dashboard startup**

The dashboard's `serve()` entry calls a small `reindex_all()` that walks the existing artifact stores once on startup, plus `seed_actions()`. After that, the indexer hooks keep things current.

- [ ] **Step 6: Commit**

```bash
git commit -am "feat(engine): per-artifact search indexers + bootstrap reindex"
```

---

## Phase B — Dashboard surface

### Task 4: GET /api/search endpoint

**File:** `crates/xvision-dashboard/src/routes/search.rs`

- [ ] **Step 1: Handler**

```rust
use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use xvision_engine::search::{self, SearchHit, SearchKind, SearchOpts};

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub kinds: Option<String>,    // comma-separated
    pub limit: Option<usize>,
}

pub async fn search_handler(
    State(state): State<crate::AppState>,
    Query(req): Query<SearchQuery>,
) -> Json<Vec<SearchHit>> {
    let idx = state.search_index.clone();
    let opts = SearchOpts {
        kinds: req.kinds.as_deref().map(parse_kinds),
        limit: req.limit,
    };
    let hits = search::search(&idx, &req.q, &opts).unwrap_or_default();
    Json(hits)
}

fn parse_kinds(s: &str) -> Vec<SearchKind> { s.split(',').filter_map(SearchKind::from_str).collect() }
```

Wire into `routes/mod.rs` as `GET /api/search`. Add `search_index: Arc<SearchIndex>` to `AppState`.

- [ ] **Step 2: Test**

```rust
#[tokio::test]
async fn search_endpoint_returns_hits() {
    let state = build_state_with_seeded_index();
    let app = build_router(state);
    let resp = app.oneshot(Request::get("/api/search?q=btc&limit=5").body(Body::empty()).unwrap()).await.unwrap();
    let hits: Vec<serde_json::Value> = serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0]["title"], "btc-momentum");
}
```

- [ ] **Step 3: Commit**

```bash
git commit -am "feat(dashboard): GET /api/search wired to FTS index"
```

---

### Task 5: Modal markup in `base.html`

**File:** `crates/xvision-dashboard/templates/base.html`

Add the `<dialog>` markup to `base.html` so it's available on every authenticated route. Per ui-elements.md §1.5: 640px wide, centered, search input at top, results grouped by kind.

- [ ] **Step 1: Markup**

```html
{# templates/base.html — append before </body> #}
<dialog id="cmd-palette" class="cmd-palette card">
  <input id="cmd-q" type="text" placeholder="Jump to a strategy, run, finding, or scenario…" autocomplete="off">
  <div id="cmd-results"></div>
  <footer class="text-xs text-tertiary px-3 py-2 border-t border-soft mono">
    ↑↓ navigate · ↵ open · esc close
  </footer>
</dialog>
```

CSS in `theme.css`:

```css
.cmd-palette {
  width: 640px;
  max-width: 90vw;
  padding: 0;
  border: 1px solid var(--border);
}
.cmd-palette::backdrop { background: rgba(0,0,0,0.5); }
#cmd-q {
  width: 100%; box-sizing: border-box;
  background: transparent; color: var(--text-primary);
  border: 0; border-bottom: 1px solid var(--border);
  padding: 14px 18px; font-size: 16px;
  outline: none;
}
#cmd-results { max-height: 400px; overflow-y: auto; }
.cmd-group-header { padding: 6px 18px; font-size: 11px; text-transform: uppercase; color: var(--text-secondary); }
.cmd-row { padding: 8px 18px; cursor: pointer; display: flex; gap: 12px; align-items: baseline; }
.cmd-row.active, .cmd-row:hover { background: var(--bg-panel); }
.cmd-row .cmd-title { flex: 1; }
.cmd-row .cmd-summary { color: var(--text-secondary); font-size: 12px; }
.cmd-row .cmd-key { color: var(--text-tertiary); font-size: 11px; font-family: 'JetBrains Mono', monospace; }
```

- [ ] **Step 2: Commit**

```bash
git commit -am "feat(dashboard): command palette modal markup in base template"
```

---

### Task 6: `command_palette.js` — bind ⌘K + render results

**File:** `crates/xvision-dashboard/static/js/command_palette.js`

- [ ] **Step 1: Open / close binding**

```javascript
const dlg = document.getElementById('cmd-palette');
const q = document.getElementById('cmd-q');
const results = document.getElementById('cmd-results');

document.addEventListener('keydown', e => {
  const meta = (navigator.platform.includes('Mac') ? e.metaKey : e.ctrlKey);
  if (meta && e.key === 'k') {
    e.preventDefault();
    if (dlg.open) closePalette(); else openPalette();
  } else if (e.key === 'Escape' && dlg.open) {
    closePalette();
  }
});

function openPalette() {
  dlg.showModal();
  q.value = '';
  results.innerHTML = '';
  q.focus();
}

function closePalette() { dlg.close(); }
```

- [ ] **Step 2: Type-to-search with debounce**

```javascript
let activeIdx = 0;
let activeRows = [];
let timer;

q.addEventListener('input', () => {
  clearTimeout(timer);
  timer = setTimeout(runSearch, 80);
});

async function runSearch() {
  const term = q.value.trim();
  if (!term) { results.innerHTML = ''; activeRows = []; return; }
  const hits = await fetch(`/api/search?q=${encodeURIComponent(term)}&limit=40`).then(r => r.json());
  render(hits);
}

const KIND_ORDER = ['action', 'strategy', 'run', 'finding', 'scenario', 'deployment', 'journal_entry'];
const KIND_LABEL = {
  action: 'Actions', strategy: 'Strategies', run: 'Runs',
  finding: 'Findings', scenario: 'Scenarios', deployment: 'Deployments',
  journal_entry: 'Journal',
};

function render(hits) {
  const grouped = {};
  for (const h of hits) (grouped[h.kind] ||= []).push(h);
  results.innerHTML = '';
  activeRows = [];
  for (const k of KIND_ORDER) {
    const group = grouped[k];
    if (!group?.length) continue;
    const header = document.createElement('div');
    header.className = 'cmd-group-header';
    header.textContent = KIND_LABEL[k];
    results.appendChild(header);
    for (const h of group) {
      const row = document.createElement('div');
      row.className = 'cmd-row';
      row.dataset.href = h.href;
      row.innerHTML = `<span class="cmd-title">${escape(h.title)}<div class="cmd-summary">${escape(h.summary)}</div></span><span class="cmd-key">↵</span>`;
      results.appendChild(row);
      activeRows.push(row);
    }
  }
  activeIdx = 0;
  highlight();
}

function highlight() {
  for (const r of activeRows) r.classList.remove('active');
  activeRows[activeIdx]?.classList.add('active');
  activeRows[activeIdx]?.scrollIntoView({block: 'nearest'});
}

q.addEventListener('keydown', e => {
  if (e.key === 'ArrowDown') { e.preventDefault(); activeIdx = Math.min(activeIdx + 1, activeRows.length - 1); highlight(); }
  else if (e.key === 'ArrowUp') { e.preventDefault(); activeIdx = Math.max(activeIdx - 1, 0); highlight(); }
  else if (e.key === 'Enter') {
    e.preventDefault();
    const row = activeRows[activeIdx];
    if (row) { window.location.href = row.dataset.href; closePalette(); }
  }
});

results.addEventListener('click', e => {
  const row = e.target.closest('.cmd-row');
  if (row) { window.location.href = row.dataset.href; closePalette(); }
});

function escape(s) { const d = document.createElement('div'); d.textContent = s; return d.innerHTML; }
```

- [ ] **Step 3: Mount script in base.html**

```html
<script type="module" src="/static/js/command_palette.js"></script>
```

- [ ] **Step 4: Smoke**

Open dashboard, press `⌘K`, type `btc` → palette shows matching strategies. Arrow down → second row highlights. Enter → navigates to `/authoring/btc-momentum`.

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(dashboard): command palette frontend with ⌘K binding"
```

---

## Phase C — Integration + smoke

### Task 7: Bootstrap reindex on dashboard startup

Modify `xvision-dashboard::serve()` to:
1. Open `SearchIndex` and stash in `AppState`.
2. Call `reindex_all(&idx, &xvn_home)` once.
3. Call `seed_actions(&idx)`.

This handles the cold-start case — a user who has artifacts but no index yet gets one populated transparently. Subsequent indexing is incremental via the per-artifact hooks.

- [ ] **Step 1: `reindex_all`**

```rust
pub fn reindex_all(idx: &SearchIndex, xvn_home: &Path) -> anyhow::Result<()> {
    // Walk strategies/, runs/, deployments/, scenarios/, journal_entries
    // For each, call the appropriate per-artifact indexer.
    // This is idempotent: upsert handles re-runs.
    Ok(())
}
```

- [ ] **Step 2: Smoke + commit**

```bash
xvn  # opens dashboard
# verify ⌘K lights up immediately, even with stale artifacts
git commit -am "feat(dashboard): bootstrap reindex on startup"
```

---

### Task 8: End-to-end smoke + README

Manual flow:
1. With seeded `XVN_HOME` (3 strategies, 2 runs, 1 finding), open `/`.
2. `⌘K` → palette opens.
3. Type `btc` → see Strategies group with `btc-momentum`. Arrow down to Runs group. Enter → navigates.
4. Type `new strat` → Actions group with `New strategy from template…`. Enter → navigates to `/setup?seed=template-picker`.
5. Type `journ` → if Lab Notebook is shipped, the most recent journal entries appear under Journal. If not, no Journal group surfaces.

Document in `crates/xvision-dashboard/README.md` under "Command Palette".

Commit `chore: command palette smoke verified`.

---

## Self-review checklist

**Spec coverage:**
- [x] §1.5 Modal overlay 640px centered — Task 5
- [x] §1.5 Search input + placeholder — Task 5
- [x] §1.5 Grouped results — Task 6 (`KIND_ORDER`)
- [x] §1.5 Action row "New strategy from template…" — Task 3 Step 4
- [x] §1.5 Findings group searchable — Task 3 Step 3 (Finding indexer) + Task 6 group rendering

**Out of scope as planned:**
- [ ] Personalized ranking — v1.1
- [ ] Body-content full-text — v1.1
- [ ] Customizable shortcut — never

**Type consistency:** `IndexEntry`, `SearchKind`, `SearchHit`, `SearchOpts` — consistent.

**Frequent commits:** 8 tasks → ~8 commits.

---

## What's next

This palette is a primitive for future work:
- The chat-rail-persistence plan can wire palette `Findings` group rows to "Draft variant from this →" actions, mirroring `Run detail`'s buttons.
- A future `/lab` notebook view (Power Notebook archetype, deferred) can use the same FTS index for cell-attached references.
- v1.1: index `body_markdown` (journal entries, finding details, prompts) when corpus size makes it worth it.
