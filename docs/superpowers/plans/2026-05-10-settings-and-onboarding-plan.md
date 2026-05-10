# Settings & Onboarding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Depends on:** Plan 2d (dashboard scaffold + axum + askama). LLM-providers design spec `docs/superpowers/specs/2026-05-10-llm-providers-and-per-arm-models-design.md` (provider registry shape). UI design lock in `docs/design/ui-elements.md` §13 + `docs/design/gptprompts.md` §17–§18.
> **Sequencing:** Ships **after** the LLM-providers backend plan (`2026-05-10-llm-providers-and-per-arm-models-plan.md`) lands the `[[providers]]` config + `xvn provider` CLI. This plan is the matching UI surface so `/settings/providers` actually exists when the dashboard lands.

**Goal:** A working `/settings` page with sidebar nav and five sections that v1 actually needs: **Providers** (LLM keys), **Brokers**, **Daemon**, **Identity** (read-only stub in v1), **Danger zone**. Plus a wired first-run flow at `/setup` so a user with no LLM key can paste one and land on the Wizard with that key persisted as a `[[providers]]` row in `config/default.toml`.

**Architecture:** New `/settings/...` route family inside `xianvec-dashboard`. The sidebar layout is one `base.html` template that every settings sub-page extends. Each sub-page is its own askama template + JS module. Mutations go through a small REST surface (`POST /api/settings/...`) that delegates to a new `xianvec-core::settings` module — the only crate that knows how to read and rewrite `config/default.toml`. The first-run flow at `/setup` reuses the same provider-add path so there's exactly one code path that creates a `[[providers]]` row.

**Tech Stack:** Rust 2021. Reuses Plan 2d's axum + askama + rust-embed setup. New deps: `toml_edit = "0.22"` in `xianvec-core` (so we can rewrite `config/default.toml` while preserving comments and field ordering). No new frontend deps — plain HTML/JS modules + Tailwind via CDN.

**Out of scope (deferred):**
- Marketplace settings sub-page (§18) — ships with Plan 5 (blockchain integration). v1 shows a placeholder card.
- Autoresearch settings sub-page (§17) — ships with the autoresearcher plan series. v1 shows a placeholder card.
- Multi-workspace switcher in Account section — v1 is single workspace per `XVN_HOME`.
- Telemetry rollups beyond a "● Connected / ○ Offline" daemon heartbeat. Real telemetry lands when the eval engine plan ships its tracing surface.
- Live Identity (ERC-8004) editing — read-only display in v1; on-chain mint/edit flows live in Plan 5.
- Light theme toggle — defer with the theme pilot.

---

## File structure

```
crates/
├── xianvec-core/
│   └── src/
│       ├── settings.rs                            # NEW: read/write config/default.toml safely
│       └── config.rs                              # MODIFY: pub fn config_path(home: &Path) -> PathBuf
├── xianvec-dashboard/
│   ├── src/routes/
│   │   ├── settings.rs                            # NEW: settings shell + sub-page handlers
│   │   ├── settings_providers.rs                  # NEW: GET + REST for /settings/providers
│   │   ├── settings_brokers.rs                    # NEW: GET + REST for /settings/brokers
│   │   ├── settings_daemon.rs                     # NEW: GET for /settings/daemon
│   │   ├── settings_identity.rs                   # NEW: GET for /settings/identity (read-only v1)
│   │   ├── settings_danger.rs                     # NEW: GET + REST for /settings/danger
│   │   └── setup.rs                               # NEW: first-run /setup wiring (was implicit in Plan 2d)
│   ├── templates/
│   │   ├── settings_base.html                     # NEW: sidebar nav layout, extends base.html
│   │   ├── settings_providers.html                # NEW
│   │   ├── settings_brokers.html                  # NEW
│   │   ├── settings_daemon.html                   # NEW
│   │   ├── settings_identity.html                 # NEW
│   │   ├── settings_danger.html                   # NEW
│   │   └── setup_first_run.html                   # NEW: Add-an-LLM-key card (replaces Plan 2d's prompt())
│   └── static/js/
│       ├── settings_providers.js                  # NEW: add/edit/delete/test provider rows
│       ├── settings_brokers.js                    # NEW
│       ├── settings_danger.js                     # NEW
│       └── setup_first_run.js                     # NEW: provider paste form
└── xianvec-cli/
    └── src/commands/
        └── (no new files — settings is dashboard-only in v1; CLI uses existing `xvn provider`)
```

---

## Phase A — Settings shell + sidebar nav

### Task 1: `xianvec-core::settings` module + `toml_edit` dep

**Files:**
- Create: `crates/xianvec-core/src/settings.rs`
- Modify: `crates/xianvec-core/Cargo.toml` (add `toml_edit`)
- Modify: `crates/xianvec-core/src/lib.rs` (re-export `settings`)
- Modify: `crates/xianvec-core/src/config.rs` (add `pub fn config_path(home: &Path) -> PathBuf`)
- Test: `crates/xianvec-core/tests/settings_roundtrip.rs`

- [ ] **Step 1: Add `toml_edit` to Cargo.toml**

```toml
# crates/xianvec-core/Cargo.toml
[dependencies]
# ...existing deps
toml_edit = "0.22"
```

- [ ] **Step 2: Define `settings.rs`**

```rust
//! Read/write `config/default.toml` while preserving comments and field
//! ordering. The dashboard `/settings` UI calls these functions directly;
//! the `xvn provider` CLI also uses them so there's one code path.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use toml_edit::{Array, DocumentMut, Item, Table, value};

use crate::config::ProviderEntry;

/// Read `config/default.toml` into a mutable document. Caller mutates and
/// passes back to `write_doc`. Round-tripping preserves whitespace +
/// comments.
pub fn read_doc(config_path: &Path) -> Result<DocumentMut> {
    let raw = std::fs::read_to_string(config_path)
        .with_context(|| format!("read {}", config_path.display()))?;
    raw.parse::<DocumentMut>()
        .with_context(|| format!("parse {} as TOML", config_path.display()))
}

pub fn write_doc(config_path: &Path, doc: &DocumentMut) -> Result<()> {
    std::fs::write(config_path, doc.to_string())
        .with_context(|| format!("write {}", config_path.display()))
}

/// Append a new `[[providers]]` row. Errors if `name` already exists or
/// starts with `_` (synthetic-namespace reservation; see LLM-providers
/// spec §4.3).
pub fn add_provider(doc: &mut DocumentMut, entry: &ProviderEntry) -> Result<()> {
    if entry.name.starts_with('_') {
        return Err(anyhow!("provider names starting with '_' are reserved"));
    }
    let arr = doc
        .entry("providers")
        .or_insert(Item::ArrayOfTables(Default::default()))
        .as_array_of_tables_mut()
        .ok_or_else(|| anyhow!("[[providers]] is not an array of tables"))?;
    if arr.iter().any(|t| t.get("name").and_then(Item::as_str) == Some(&entry.name)) {
        return Err(anyhow!("provider {} already exists", entry.name));
    }
    let mut t = Table::new();
    t["name"] = value(entry.name.clone());
    t["kind"] = value(entry.kind.as_str());      // ProviderKind::as_str — add if missing
    t["base_url"] = value(entry.base_url.clone());
    t["api_key_env"] = value(entry.api_key_env.clone());
    arr.push(t);
    Ok(())
}

pub fn remove_provider(doc: &mut DocumentMut, name: &str) -> Result<()> {
    let arr = doc
        .get_mut("providers")
        .and_then(Item::as_array_of_tables_mut)
        .ok_or_else(|| anyhow!("no [[providers]] array to remove from"))?;
    let before = arr.len();
    arr.retain(|t| t.get("name").and_then(Item::as_str) != Some(name));
    if arr.len() == before {
        return Err(anyhow!("provider {name} not found"));
    }
    Ok(())
}

pub fn update_provider(
    doc: &mut DocumentMut,
    name: &str,
    new: &ProviderEntry,
) -> Result<()> {
    let arr = doc
        .get_mut("providers")
        .and_then(Item::as_array_of_tables_mut)
        .ok_or_else(|| anyhow!("no [[providers]] array"))?;
    let row = arr
        .iter_mut()
        .find(|t| t.get("name").and_then(Item::as_str) == Some(name))
        .ok_or_else(|| anyhow!("provider {name} not found"))?;
    row["name"] = value(new.name.clone());
    row["kind"] = value(new.kind.as_str());
    row["base_url"] = value(new.base_url.clone());
    row["api_key_env"] = value(new.api_key_env.clone());
    Ok(())
}
```

- [ ] **Step 3: Add `config_path` helper**

```rust
// crates/xianvec-core/src/config.rs
pub fn config_path(home: &std::path::Path) -> std::path::PathBuf {
    home.join("config").join("default.toml")
}
```

- [ ] **Step 4: Roundtrip test**

```rust
// crates/xianvec-core/tests/settings_roundtrip.rs
use xianvec_core::config::{ProviderEntry, ProviderKind};
use xianvec_core::settings::{add_provider, read_doc, remove_provider, write_doc};

#[test]
fn add_then_remove_preserves_comments() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.toml");
    std::fs::write(&path, r#"
# user-managed config
[runtime]
mode = "backtest"

[[providers]]
name = "anthropic"
kind = "anthropic"
base_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"
"#).unwrap();

    let mut doc = read_doc(&path).unwrap();
    let entry = ProviderEntry {
        name: "openai".into(),
        kind: ProviderKind::OpenaiCompat,
        base_url: "https://api.openai.com/v1".into(),
        api_key_env: "OPENAI_API_KEY".into(),
    };
    add_provider(&mut doc, &entry).unwrap();
    write_doc(&path, &doc).unwrap();

    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("# user-managed config"));   // comment preserved
    assert!(raw.contains(r#"name = "openai""#));

    let mut doc = read_doc(&path).unwrap();
    remove_provider(&mut doc, "openai").unwrap();
    write_doc(&path, &doc).unwrap();

    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(!raw.contains("openai"));
    assert!(raw.contains("anthropic"));               // didn't nuke siblings
}

#[test]
fn rejects_underscore_prefix_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.toml");
    std::fs::write(&path, "[runtime]\nmode = \"backtest\"\n").unwrap();
    let mut doc = read_doc(&path).unwrap();
    let err = add_provider(&mut doc, &ProviderEntry {
        name: "_synth".into(),
        kind: ProviderKind::Anthropic,
        base_url: "https://x".into(),
        api_key_env: "X".into(),
    }).unwrap_err();
    assert!(err.to_string().contains("reserved"));
}
```

Run: `cargo test -p xianvec-core --test settings_roundtrip` — expect 2 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-core
git commit -m "feat(core): settings module with toml_edit roundtrip for [[providers]] rows"
```

---

### Task 2: Settings shell — sidebar template + base route

**Files:**
- Create: `crates/xianvec-dashboard/templates/settings_base.html`
- Create: `crates/xianvec-dashboard/src/routes/settings.rs`
- Modify: `crates/xianvec-dashboard/src/routes/mod.rs` (mount `/settings/...`)

- [ ] **Step 1: settings_base.html — sidebar layout**

Per `gptprompts.md` §17 layout: 220px sidebar nav + content flex. Active row gets a 2px mint left bar (per dashboard `theme.css`).

```html
{# templates/settings_base.html #}
{% extends "base.html" %}
{% block title %}xvn — Settings — {{ section_title }}{% endblock %}
{% block main %}
<div class="grid grid-cols-[220px_1fr] gap-6 max-w-6xl mx-auto">
  <nav class="card p-4 h-fit sticky top-6">
    <div class="text-xs uppercase text-secondary mb-2">Account</div>
    <a href="/settings/account" class="settings-row {{ 'active' if section == 'account' else '' }}">Account</a>
    <a href="/settings/appearance" class="settings-row {{ 'active' if section == 'appearance' else '' }}">Appearance</a>
    <hr class="my-3 border-soft">
    <div class="text-xs uppercase text-secondary mb-2">Config</div>
    <a href="/settings/providers" class="settings-row {{ 'active' if section == 'providers' else '' }}">Providers</a>
    <a href="/settings/brokers" class="settings-row {{ 'active' if section == 'brokers' else '' }}">Brokers</a>
    <a href="/settings/autoresearch" class="settings-row {{ 'active' if section == 'autoresearch' else '' }}">Autoresearch <span class="pill text-xs ml-2">soon</span></a>
    <a href="/settings/marketplace" class="settings-row {{ 'active' if section == 'marketplace' else '' }}">Marketplace <span class="pill text-xs ml-2">soon</span></a>
    <a href="/settings/identity" class="settings-row pl-6 {{ 'active' if section == 'identity' else '' }}">└ Identity</a>
    <hr class="my-3 border-soft">
    <div class="text-xs uppercase text-secondary mb-2">Runtime</div>
    <a href="/settings/daemon" class="settings-row {{ 'active' if section == 'daemon' else '' }}">Daemon</a>
    <a href="/settings/telemetry" class="settings-row {{ 'active' if section == 'telemetry' else '' }}">Telemetry</a>
    <hr class="my-3 border-soft">
    <a href="/settings/danger" class="settings-row danger {{ 'active' if section == 'danger' else '' }}">Danger zone</a>
  </nav>
  <section>
    <header class="mb-6">
      <h1 class="text-2xl">{{ section_title }}</h1>
      {% if section_subtitle %}<p class="text-secondary text-sm mt-1">{{ section_subtitle }}</p>{% endif %}
    </header>
    {% block settings_content %}{% endblock %}
  </section>
</div>
{% endblock %}
```

Append to `static/css/theme.css`:

```css
.settings-row {
  display: block;
  padding: 6px 10px;
  border-radius: 6px;
  color: var(--text-primary);
  text-decoration: none;
  font-size: 14px;
  border-left: 2px solid transparent;
}
.settings-row:hover { background: var(--bg-panel); }
.settings-row.active {
  background: var(--bg-panel);
  border-left-color: var(--accent-mint);
}
.settings-row.danger { color: var(--status-danger); }
```

- [ ] **Step 2: routes/settings.rs — index + redirect**

```rust
use askama::Template;
use askama_axum::IntoResponse;
use axum::response::Redirect;

pub async fn index() -> Redirect {
    Redirect::permanent("/settings/providers")
}

#[derive(Template)]
#[template(path = "settings_base.html")]
pub struct SettingsBase {
    pub section: &'static str,
    pub section_title: &'static str,
    pub section_subtitle: Option<&'static str>,
}
```

- [ ] **Step 3: Mount routes**

```rust
// routes/mod.rs
.route("/settings", get(settings::index))
.route("/settings/providers", get(settings_providers::page))
.route("/api/settings/providers", get(settings_providers::list).post(settings_providers::add))
.route("/api/settings/providers/:name", axum::routing::put(settings_providers::update).delete(settings_providers::delete))
.route("/api/settings/providers/:name/test", axum::routing::post(settings_providers::test))
.route("/settings/brokers", get(settings_brokers::page))
.route("/settings/daemon", get(settings_daemon::page))
.route("/settings/identity", get(settings_identity::page))
.route("/settings/danger", get(settings_danger::page))
.route("/settings/autoresearch", get(settings::placeholder_autoresearch))
.route("/settings/marketplace", get(settings::placeholder_marketplace))
.route("/settings/account", get(settings::placeholder_account))
.route("/settings/appearance", get(settings::placeholder_appearance))
.route("/settings/telemetry", get(settings::placeholder_telemetry))
```

The placeholder handlers render `settings_base.html` with a body of "Ships with Plan 5" / "Ships with autoresearch plan" — see Task 9 for exact copy.

- [ ] **Step 4: Smoke test**

```rust
// extend tests/routes_smoke.rs
#[tokio::test]
async fn settings_index_redirects_to_providers() {
    let app = build_router(state());
    let resp = app.oneshot(Request::get("/settings").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), 308);
    assert_eq!(resp.headers()["location"], "/settings/providers");
}
```

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-dashboard
git commit -m "feat(dashboard): /settings shell with sidebar nav + section routing"
```

---

## Phase B — Providers section (the load-bearing surface)

### Task 3: GET /settings/providers — list view

**Files:**
- Create: `crates/xianvec-dashboard/src/routes/settings_providers.rs`
- Create: `crates/xianvec-dashboard/templates/settings_providers.html`
- Create: `crates/xianvec-dashboard/static/js/settings_providers.js`

Per LLM-providers spec §7.1: sortable table, columns `Name`, `Kind`, `Base URL`, `API key env`, `Key`, `Used by`, `Actions`. Empty state has the same three quick-link buttons (Get an Anthropic key / Get an OpenAI key / Get an OpenRouter key) as the first-run modal.

- [ ] **Step 1: settings_providers.html**

```html
{% extends "settings_base.html" %}
{% block settings_content %}
<div class="flex justify-between items-center mb-4">
  <div></div>
  <button id="add-provider-btn" class="btn-primary">+ Add provider</button>
</div>
<table class="w-full card p-0" id="providers-table">
  <thead>
    <tr class="text-left text-xs uppercase text-secondary">
      <th class="px-4 py-3">Name</th>
      <th class="px-4 py-3">Kind</th>
      <th class="px-4 py-3">Base URL</th>
      <th class="px-4 py-3">API key env</th>
      <th class="px-4 py-3">Key</th>
      <th class="px-4 py-3">Used by</th>
      <th class="px-4 py-3"></th>
    </tr>
  </thead>
  <tbody id="providers-tbody"></tbody>
</table>
<div id="empty-state" class="card mt-6 hidden">
  <p class="text-secondary mb-4">No providers yet. Add Anthropic, OpenAI, or any OpenAI-compatible endpoint.</p>
  <div class="flex gap-3">
    <a class="btn-ghost" href="https://console.anthropic.com/" target="_blank">Get an Anthropic key →</a>
    <a class="btn-ghost" href="https://platform.openai.com/api-keys" target="_blank">Get an OpenAI key →</a>
    <a class="btn-ghost" href="https://openrouter.ai/keys" target="_blank">Get an OpenRouter key →</a>
  </div>
</div>

<dialog id="add-provider-modal" class="card max-w-lg p-6">
  <h2 class="mb-4">Add provider</h2>
  <form id="add-provider-form">
    <label class="block mb-3">Name <input name="name" pattern="[a-z0-9-]+" required class="w-full bg-panel border border-soft rounded px-2 py-1"></label>
    <label class="block mb-3">Kind
      <select name="kind" class="w-full bg-panel border border-soft rounded px-2 py-1">
        <option value="anthropic">anthropic</option>
        <option value="openai-compat">openai-compat</option>
        <option value="local-candle">local-candle</option>
      </select>
    </label>
    <label class="block mb-3">Base URL <input name="base_url" type="url" required class="w-full bg-panel border border-soft rounded px-2 py-1"></label>
    <label class="block mb-3">API key env
      <input name="api_key_env" placeholder="e.g. OPENAI_API_KEY" class="w-full bg-panel border border-soft rounded px-2 py-1">
      <button type="button" id="detect-env-btn" class="btn-ghost text-xs mt-1">Detect</button>
    </label>
    <div class="flex gap-2 justify-end mt-4">
      <button type="button" class="btn-ghost" onclick="document.getElementById('add-provider-modal').close()">Cancel</button>
      <button type="button" id="test-conn-btn" class="btn-ghost">Test connection</button>
      <button type="submit" class="btn-primary">Save</button>
    </div>
  </form>
</dialog>
{% endblock %}
{% block scripts %}<script type="module" src="/static/js/settings_providers.js"></script>{% endblock %}
```

- [ ] **Step 2: routes/settings_providers.rs — list endpoint**

```rust
use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse as AxumIntoResponse, Json, Response};
use serde::{Deserialize, Serialize};
use xianvec_core::config::{ProviderEntry, ProviderKind};
use xianvec_core::settings;

use crate::AppState;

#[derive(Template)]
#[template(path = "settings_providers.html")]
pub struct ProvidersPage {
    pub section: &'static str,
    pub section_title: &'static str,
    pub section_subtitle: Option<&'static str>,
}

pub async fn page() -> Response {
    ProvidersPage {
        section: "providers",
        section_title: "Providers",
        section_subtitle: Some("LLM endpoints used by every strategy and the setup agent."),
    }
    .into_response()
}

#[derive(Serialize)]
pub struct ProviderRow {
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub api_key_env: String,
    pub key_status: KeyStatus,    // {"set" | "missing" | "n/a"}
    pub used_by: Vec<String>,     // slot refs ("workspace default Intern", "draft btc-momentum.trader")
    pub synthetic: bool,           // true iff name starts with `_`
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyStatus { Set, Missing, NotApplicable }

pub async fn list(State(state): State<AppState>) -> Json<Vec<ProviderRow>> {
    let cfg_path = xianvec_core::config::config_path(&state.xvn_home);
    let cfg = xianvec_core::config::RuntimeConfig::load(&cfg_path).unwrap_or_default();
    let mut rows = Vec::with_capacity(cfg.providers.len());
    for p in &cfg.providers {
        let key_status = if p.api_key_env.is_empty() {
            KeyStatus::NotApplicable
        } else if std::env::var(&p.api_key_env).is_ok() {
            KeyStatus::Set
        } else {
            KeyStatus::Missing
        };
        rows.push(ProviderRow {
            name: p.name.clone(),
            kind: p.kind.as_str().into(),
            base_url: p.base_url.clone(),
            api_key_env: p.api_key_env.clone(),
            key_status,
            used_by: collect_slot_refs(&cfg, &p.name),
            synthetic: p.name.starts_with('_'),
        });
    }
    Json(rows)
}

fn collect_slot_refs(cfg: &xianvec_core::config::RuntimeConfig, name: &str) -> Vec<String> {
    let mut out = vec![];
    if cfg.intern.provider.as_str() == cfg.intern_kind_name(name) {
        out.push("workspace default Intern".into());
    }
    // TODO: when slot-shaped strategies land (Plan 2a), iterate drafts and
    // collect any slot referencing this provider name. For v1, only the
    // [intern] block contributes.
    out
}
```

- [ ] **Step 3: settings_providers.js — render rows**

```javascript
const tbody = document.getElementById('providers-tbody');
const empty = document.getElementById('empty-state');
const modal = document.getElementById('add-provider-modal');

async function refresh() {
  const rows = await fetch('/api/settings/providers').then(r => r.json());
  tbody.innerHTML = '';
  if (rows.length === 0) { empty.classList.remove('hidden'); return; }
  empty.classList.add('hidden');
  for (const r of rows) {
    const tr = document.createElement('tr');
    tr.className = 'border-t border-soft';
    tr.innerHTML = `
      <td class="px-4 py-2 mono">${r.name}${r.synthetic ? ' <span class="pill">synthetic</span>' : ''}</td>
      <td class="px-4 py-2">${r.kind}</td>
      <td class="px-4 py-2 mono text-secondary">${r.base_url}</td>
      <td class="px-4 py-2 mono">${r.api_key_env || '(none)'}</td>
      <td class="px-4 py-2">${keyChip(r.key_status)}</td>
      <td class="px-4 py-2 text-secondary text-xs">${r.used_by.length === 0 ? '—' : r.used_by.join(', ')}</td>
      <td class="px-4 py-2 text-right">
        <button class="btn-ghost text-xs" data-act="test" data-name="${r.name}">Test</button>
        <button class="btn-ghost text-xs" data-act="edit" data-name="${r.name}" ${r.synthetic ? 'disabled' : ''}>Edit</button>
        <button class="btn-ghost text-xs" data-act="delete" data-name="${r.name}" ${r.used_by.length > 0 || r.synthetic ? 'disabled' : ''}>Delete</button>
      </td>`;
    tbody.appendChild(tr);
  }
}

function keyChip(status) {
  if (status === 'set') return '<span class="pill" style="color: var(--accent-mint)">● set</span>';
  if (status === 'missing') return '<span class="pill" style="color: var(--status-warn)">○ missing</span>';
  return '<span class="pill text-secondary">n/a</span>';
}

document.getElementById('add-provider-btn').onclick = () => modal.showModal();

document.getElementById('add-provider-form').addEventListener('submit', async e => {
  e.preventDefault();
  const fd = new FormData(e.target);
  const body = Object.fromEntries(fd.entries());
  const resp = await fetch('/api/settings/providers', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (resp.ok) { modal.close(); refresh(); }
  else alert(`Add failed: ${await resp.text()}`);
});

tbody.addEventListener('click', async e => {
  const btn = e.target.closest('button[data-act]');
  if (!btn) return;
  const name = btn.dataset.name;
  if (btn.dataset.act === 'delete' && confirm(`Delete provider "${name}"?`)) {
    const r = await fetch(`/api/settings/providers/${name}`, { method: 'DELETE' });
    if (r.ok) refresh(); else alert(await r.text());
  } else if (btn.dataset.act === 'test') {
    btn.disabled = true; btn.textContent = '…';
    const r = await fetch(`/api/settings/providers/${name}/test`, { method: 'POST' });
    btn.disabled = false; btn.textContent = r.ok ? '✓' : '✗';
    setTimeout(() => { btn.textContent = 'Test'; }, 2000);
  }
});

refresh();
```

- [ ] **Step 4: Test — list endpoint returns rows**

```rust
// crates/xianvec-dashboard/tests/settings_providers.rs
#[tokio::test]
async fn list_returns_seeded_providers() {
    let dir = tempfile::tempdir().unwrap();
    seed_config(dir.path(), r#"
[runtime]
mode = "backtest"

[[providers]]
name = "anthropic"
kind = "anthropic"
base_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"

[intern]
provider = "anthropic"
base_url = "https://api.anthropic.com"
model = "claude-haiku-4-5"
api_key_env = "ANTHROPIC_API_KEY"
temperature = 0.0
reasoning_effort = "low"
max_tokens = 1024
"#);
    let app = build_router(AppState { xvn_home: dir.path().to_path_buf() });
    let resp = app.oneshot(Request::get("/api/settings/providers").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap()).unwrap();
    assert_eq!(body[0]["name"], "anthropic");
}
```

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-dashboard
git commit -m "feat(dashboard): /settings/providers list view + GET endpoint"
```

---

### Task 4: POST/PUT/DELETE provider endpoints

**File:** `crates/xianvec-dashboard/src/routes/settings_providers.rs`

- [ ] **Step 1: AddRequest + handler**

```rust
#[derive(Deserialize)]
pub struct AddProviderRequest {
    pub name: String,
    pub kind: String,           // "anthropic" | "openai-compat" | "local-candle"
    pub base_url: String,
    pub api_key_env: String,
}

pub async fn add(
    State(state): State<AppState>,
    Json(req): Json<AddProviderRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let entry = build_entry(&req).map_err(bad_req)?;
    let cfg_path = xianvec_core::config::config_path(&state.xvn_home);
    let mut doc = settings::read_doc(&cfg_path).map_err(internal)?;
    settings::add_provider(&mut doc, &entry).map_err(bad_req)?;
    settings::write_doc(&cfg_path, &doc).map_err(internal)?;
    Ok(StatusCode::CREATED)
}

pub async fn update(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<AddProviderRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let entry = build_entry(&req).map_err(bad_req)?;
    let cfg_path = xianvec_core::config::config_path(&state.xvn_home);
    let mut doc = settings::read_doc(&cfg_path).map_err(internal)?;
    settings::update_provider(&mut doc, &name, &entry).map_err(bad_req)?;
    settings::write_doc(&cfg_path, &doc).map_err(internal)?;
    Ok(StatusCode::OK)
}

pub async fn delete(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let cfg_path = xianvec_core::config::config_path(&state.xvn_home);
    let cfg = xianvec_core::config::RuntimeConfig::load(&cfg_path).map_err(internal)?;
    if cfg.intern_references(&name) {
        return Err((StatusCode::CONFLICT, format!(
            "cannot remove provider {name}: referenced by [intern]. Change the workspace default Intern slot first."
        )));
    }
    let mut doc = settings::read_doc(&cfg_path).map_err(internal)?;
    settings::remove_provider(&mut doc, &name).map_err(bad_req)?;
    settings::write_doc(&cfg_path, &doc).map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

fn build_entry(req: &AddProviderRequest) -> anyhow::Result<ProviderEntry> {
    Ok(ProviderEntry {
        name: req.name.clone(),
        kind: ProviderKind::parse(&req.kind)?,
        base_url: req.base_url.clone(),
        api_key_env: req.api_key_env.clone(),
    })
}

fn bad_req(e: impl std::fmt::Display) -> (StatusCode, String) { (StatusCode::BAD_REQUEST, e.to_string()) }
fn internal(e: impl std::fmt::Display) -> (StatusCode, String) { (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()) }
```

- [ ] **Step 2: Test — add then list shows the new row**

```rust
#[tokio::test]
async fn add_then_list_includes_new_row() {
    let dir = tempfile::tempdir().unwrap();
    seed_config(dir.path(), MINIMAL_CONFIG);
    let app = build_router(AppState { xvn_home: dir.path().to_path_buf() });
    let body = serde_json::json!({"name": "openai", "kind": "openai-compat",
        "base_url": "https://api.openai.com/v1", "api_key_env": "OPENAI_API_KEY"});
    let resp = app.clone().oneshot(
        Request::post("/api/settings/providers")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string())).unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), 201);

    let resp = app.oneshot(Request::get("/api/settings/providers").body(Body::empty()).unwrap()).await.unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap()).unwrap();
    assert!(rows.iter().any(|r| r["name"] == "openai"));
}
```

- [ ] **Step 3: Test — delete refuses when [intern] references it**

```rust
#[tokio::test]
async fn delete_refuses_provider_referenced_by_intern() {
    let dir = tempfile::tempdir().unwrap();
    seed_config(dir.path(), MINIMAL_CONFIG);   // [intern].provider = "anthropic"
    let app = build_router(AppState { xvn_home: dir.path().to_path_buf() });
    let resp = app.oneshot(
        Request::delete("/api/settings/providers/anthropic").body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), 409);
}
```

- [ ] **Step 4: Commit**

```bash
git add crates/xianvec-dashboard
git commit -m "feat(dashboard): provider add/update/delete endpoints with intern-ref guard"
```

---

### Task 5: Test connection endpoint

**File:** `crates/xianvec-dashboard/src/routes/settings_providers.rs`

Per LLM-providers spec §10 open question: TCP-connect by default. v1 implementation: try `reqwest::get(base_url)` with 3-second timeout, surface OK / err string. (Real `/models` ping is a `--probe` flag in the CLI; the UI sticks with TCP-level.)

- [ ] **Step 1: Handler**

```rust
pub async fn test(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let cfg = xianvec_core::config::RuntimeConfig::load(
        &xianvec_core::config::config_path(&state.xvn_home)
    ).map_err(internal)?;
    let p = cfg.providers.iter().find(|p| p.name == name)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("provider {name} not found")))?;
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(3)).build().map_err(internal)?;
    match client.get(&p.base_url).send().await {
        Ok(resp) => Ok(Json(serde_json::json!({"ok": true, "status": resp.status().as_u16()}))),
        Err(e)   => Ok(Json(serde_json::json!({"ok": false, "error": e.to_string()}))),
    }
}
```

- [ ] **Step 2: Smoke**

```bash
curl -X POST http://127.0.0.1:7878/api/settings/providers/anthropic/test
# → {"ok": true, "status": 200} or {"ok": true, "status": 404} (any 4xx is fine — endpoint is reachable)
```

- [ ] **Step 3: Commit**

```bash
git commit -am "feat(dashboard): provider test-connection endpoint (TCP-level)"
```

---

## Phase C — First-run flow

### Task 6: `/setup` first-run-key card

**Files:**
- Create: `crates/xianvec-dashboard/templates/setup_first_run.html`
- Create: `crates/xianvec-dashboard/src/routes/setup.rs`
- Create: `crates/xianvec-dashboard/static/js/setup_first_run.js`
- Modify: `crates/xianvec-dashboard/src/routes/wizard.rs` (redirect to `/setup` when no provider with set key)
- Modify: `crates/xianvec-dashboard/static/js/wizard.js` (remove the inline `prompt()` from Plan 2d Task 7 — provider+key is now persisted server-side)

This task replaces Plan 2d's `localStorage.getItem('xvn_anthropic_key') || prompt(...)` hack with a real first-run page that writes a `[[providers]]` row and a `[intern]` block.

- [ ] **Step 1: First-run detection in wizard root handler**

```rust
// routes/wizard.rs — modify root()
pub async fn root(State(state): State<AppState>) -> Response {
    let cfg = xianvec_core::config::RuntimeConfig::load(
        &xianvec_core::config::config_path(&state.xvn_home)
    ).unwrap_or_default();
    let has_set_key = cfg.providers.iter().any(|p|
        !p.api_key_env.is_empty() && std::env::var(&p.api_key_env).is_ok()
    );
    if !has_set_key {
        return axum::response::Redirect::temporary("/setup").into_response();
    }
    WizardPage.into_response()
}
```

The first-run predicate matches `ui-elements.md` §18 open question 1: redirect to `/setup` when there is no provider whose `api_key_env` is set in the environment.

- [ ] **Step 2: setup_first_run.html (per ui-elements.md §3.3)**

```html
{% extends "base.html" %}
{% block main %}
<div class="grid grid-cols-[58%_42%] gap-6 max-w-6xl mx-auto">
  <section class="card p-6">
    <h2 class="text-2xl mb-2">Add an LLM key to begin</h2>
    <p class="text-secondary mb-6">xvn uses your key for both the setup agent and the strategies it builds. We never store your key on a server — it lives in your shell environment, and only the env-var <em>name</em> goes in <code class="mono">config/default.toml</code>.</p>
    <div class="grid grid-cols-3 gap-3 mb-6">
      <a class="btn-ghost text-center" href="https://console.anthropic.com/" target="_blank">Get an Anthropic key →</a>
      <a class="btn-ghost text-center" href="https://platform.openai.com/api-keys" target="_blank">Get an OpenAI key →</a>
      <a class="btn-ghost text-center" href="https://openrouter.ai/keys" target="_blank">Get an OpenRouter key →</a>
    </div>
    <form id="first-run-form">
      <label class="block mb-3">Already have one?
        <input name="key" type="password" placeholder="sk-…" required class="w-full bg-panel border border-soft rounded px-2 py-1">
      </label>
      <label class="block mb-3">Provider
        <select name="provider" class="w-full bg-panel border border-soft rounded px-2 py-1">
          <option value="anthropic">Anthropic (default)</option>
          <option value="openai">OpenAI</option>
          <option value="openrouter">OpenRouter</option>
        </select>
      </label>
      <button type="submit" class="btn-primary w-full">Save and continue</button>
    </form>
    <p class="text-secondary text-xs mt-4"><a href="/docs/why-no-server-keys" class="hover:text-mint">Why we don't issue keys →</a></p>
  </section>
  <aside class="card p-6 bg-panel">
    <div class="text-xs uppercase text-secondary mb-3">What happens when you save</div>
    <ol class="text-sm space-y-2 list-decimal pl-4">
      <li>Your key is written to <code class="mono">~/.xvn/secrets.env</code> as <code class="mono">ANTHROPIC_API_KEY=…</code> (mode 0600).</li>
      <li>A <code class="mono">[[providers]]</code> row is added to <code class="mono">config/default.toml</code>.</li>
      <li>The <code class="mono">[intern]</code> block is updated to point at the new provider with the default model.</li>
      <li>You're redirected to the Wizard.</li>
    </ol>
    <p class="text-secondary text-xs mt-4">You can change all of this later in <a href="/settings/providers" class="hover:text-mint">Settings → Providers</a>.</p>
  </aside>
</div>
{% endblock %}
{% block scripts %}<script type="module" src="/static/js/setup_first_run.js"></script>{% endblock %}
```

- [ ] **Step 3: setup.rs — first-run page + `/api/setup/first-run` handler**

```rust
use axum::extract::{Json, State};
use axum::http::StatusCode;
use serde::Deserialize;

use xianvec_core::config::{ProviderEntry, ProviderKind};
use xianvec_core::settings;

#[derive(Deserialize)]
pub struct FirstRunRequest {
    pub key: String,
    pub provider: String,    // "anthropic" | "openai" | "openrouter"
}

pub async fn first_run(
    State(state): State<crate::AppState>,
    Json(req): Json<FirstRunRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let preset = preset_for(&req.provider).ok_or((StatusCode::BAD_REQUEST, format!("unknown provider {}", req.provider)))?;
    write_secret(&state.xvn_home, &preset.env_var, &req.key)?;
    let cfg_path = xianvec_core::config::config_path(&state.xvn_home);
    let mut doc = settings::read_doc(&cfg_path).unwrap_or_else(|_| settings::default_doc());
    let entry = ProviderEntry {
        name: preset.name.into(),
        kind: preset.kind,
        base_url: preset.base_url.into(),
        api_key_env: preset.env_var.into(),
    };
    let _ = settings::add_provider(&mut doc, &entry);   // ignore "already exists"
    settings::set_intern_provider(&mut doc, preset.name, preset.default_model)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    settings::write_doc(&cfg_path, &doc)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::CREATED)
}

fn write_secret(xvn_home: &std::path::Path, env_var: &str, value: &str) -> Result<(), (StatusCode, String)> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let path = xvn_home.join("secrets.env");
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?; }
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut updated = existing.lines().filter(|l| !l.starts_with(&format!("{env_var}="))).collect::<Vec<_>>().join("\n");
    if !updated.is_empty() { updated.push('\n'); }
    updated.push_str(&format!("{env_var}={value}\n"));
    let mut f = std::fs::OpenOptions::new().create(true).truncate(true).write(true).mode(0o600)
        .open(&path).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    f.write_all(updated.as_bytes()).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(())
}

struct Preset { name: &'static str, kind: ProviderKind, base_url: &'static str, env_var: &'static str, default_model: &'static str }
fn preset_for(p: &str) -> Option<Preset> {
    Some(match p {
        "anthropic"  => Preset { name: "anthropic",  kind: ProviderKind::Anthropic,    base_url: "https://api.anthropic.com",    env_var: "ANTHROPIC_API_KEY",  default_model: "claude-haiku-4-5" },
        "openai"     => Preset { name: "openai",     kind: ProviderKind::OpenaiCompat, base_url: "https://api.openai.com/v1",    env_var: "OPENAI_API_KEY",     default_model: "gpt-4o-mini" },
        "openrouter" => Preset { name: "openrouter", kind: ProviderKind::OpenaiCompat, base_url: "https://openrouter.ai/api/v1", env_var: "OPENROUTER_API_KEY", default_model: "anthropic/claude-haiku-4-5" },
        _ => return None,
    })
}
```

- [ ] **Step 4: setup_first_run.js**

```javascript
document.getElementById('first-run-form').addEventListener('submit', async e => {
  e.preventDefault();
  const fd = new FormData(e.target);
  const body = Object.fromEntries(fd.entries());
  const resp = await fetch('/api/setup/first-run', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (resp.status === 201) {
    alert('Saved. Reloading.');
    window.location.href = '/';
  } else {
    alert(`Save failed: ${await resp.text()}`);
  }
});
```

- [ ] **Step 5: Daemon reads `secrets.env` on startup**

The dashboard `serve()` entry point should `dotenvy::from_path(xvn_home.join("secrets.env"))?` before reading any provider env vars. Add `dotenvy = "0.15"` to xianvec-dashboard.

```rust
// lib.rs serve()
let _ = dotenvy::from_path(xvn_home.join("secrets.env"));
```

- [ ] **Step 6: End-to-end test**

```rust
#[tokio::test]
async fn first_run_writes_provider_and_secret() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("config")).unwrap();
    std::fs::write(dir.path().join("config/default.toml"), "[runtime]\nmode = \"backtest\"\n").unwrap();
    let app = build_router(AppState { xvn_home: dir.path().to_path_buf() });
    let body = serde_json::json!({"key": "sk-test-123", "provider": "anthropic"});
    let resp = app.oneshot(
        Request::post("/api/setup/first-run")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string())).unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), 201);
    let cfg = std::fs::read_to_string(dir.path().join("config/default.toml")).unwrap();
    assert!(cfg.contains(r#"name = "anthropic""#));
    let secrets = std::fs::read_to_string(dir.path().join("secrets.env")).unwrap();
    assert!(secrets.contains("ANTHROPIC_API_KEY=sk-test-123"));
}
```

- [ ] **Step 7: Commit**

```bash
git add crates/xianvec-dashboard
git commit -m "feat(dashboard): /setup first-run flow writes provider + secrets.env + redirects"
```

---

## Phase D — Brokers, Daemon, Identity, Danger

### Task 7: `/settings/brokers` (Alpaca + Orderly)

**Files:**
- Create: `crates/xianvec-dashboard/templates/settings_brokers.html`
- Create: `crates/xianvec-dashboard/src/routes/settings_brokers.rs`
- Create: `crates/xianvec-dashboard/static/js/settings_brokers.js`

V1 scope: **Alpaca** (paper + live key paste) and **Orderly** (registration stub — actual wallet flow ships with non-custodial-wallets plan). Same TOML-edit pattern as providers, but writes `[[brokers]]` rows.

- [ ] **Step 1: Define `[[brokers]]` schema in `xianvec-core::config`**

```rust
// crates/xianvec-core/src/config.rs
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate)]
pub struct BrokerEntry {
    #[garde(length(min=1, max=32), pattern(r"^[a-z0-9-]+$"))]
    pub name: String,
    #[garde(skip)]
    pub kind: BrokerKind,           // Alpaca | Orderly | Stub
    #[garde(length(min=1, max=512))]
    pub base_url: String,
    #[garde(length(max=64))]
    pub key_id_env: String,         // e.g. "ALPACA_KEY_ID"
    #[garde(length(max=64))]
    pub secret_env: String,         // e.g. "ALPACA_SECRET_KEY"
    pub paper: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrokerKind { Alpaca, Orderly, Stub }
```

Add `RuntimeConfig.brokers: Vec<BrokerEntry>`.

- [ ] **Step 2: Brokers settings module functions**

Mirror provider settings (`add_broker`, `remove_broker`, `update_broker`) — same `toml_edit` pattern.

- [ ] **Step 3: Page template + endpoints**

Same shape as `settings_providers.html` — table with columns `Name`, `Kind`, `Mode (paper/live)`, `Key ID env`, `Secret env`, `Status`, `Actions`. Add modal asks for name, kind (radio: Alpaca / Orderly), paper toggle, key-id env, secret env. Submit writes the row.

For Orderly entries the modal also surfaces an info chip: `Wallet flow not yet wired — see non-custodial-wallets plan` and disables the submit button. (Stub the row form; skeleton only in v1 since the actual Orderly wallet flow lands in `2026-05-10-blockchain-1-non-custodial-wallets-plan.md`.)

- [ ] **Step 4: Test — add Alpaca paper broker round-trips**

```rust
#[tokio::test]
async fn add_alpaca_paper_broker() {
    let dir = tempfile::tempdir().unwrap();
    seed_config(dir.path(), MINIMAL_CONFIG);
    let app = build_router(AppState { xvn_home: dir.path().to_path_buf() });
    let body = serde_json::json!({"name": "alpaca-paper", "kind": "alpaca",
        "base_url": "https://paper-api.alpaca.markets/v2",
        "key_id_env": "ALPACA_PAPER_KEY_ID", "secret_env": "ALPACA_PAPER_SECRET_KEY",
        "paper": true});
    let resp = app.oneshot(
        Request::post("/api/settings/brokers")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string())).unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), 201);
}
```

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(dashboard): /settings/brokers with Alpaca add + Orderly stub"
```

---

### Task 8: `/settings/daemon` + `/settings/identity` + `/settings/danger`

These three are smaller — each is one page with a fixed body (no modals).

- [ ] **Step 1: `/settings/daemon` — read-only display + restart action**

Body shows: daemon PID (from `xianvec-core::daemon_pidfile`), uptime, log level (read from `XVN_LOG`), heartbeat status. Single action: `Restart daemon` button calling `POST /api/daemon/restart`. The actual restart endpoint can be a v1.1 follow-up — for now stub returns 501 with a "use `xvn daemon restart` from the CLI" message.

- [ ] **Step 2: `/settings/identity` — read-only ERC-8004 display**

Body shows the agent's ERC-8004 identity if it exists (`tokenId`, on-chain hash, Mantle explorer link). When no identity is minted, shows a card: `Identity not yet minted — see Plan 5 (blockchain integration).` with a `Read about ERC-8004` link to `docs/erc-8004-agent-uses.md`. Editing/minting flows ship in Plan 5.

- [ ] **Step 3: `/settings/danger` — wipe artifacts**

Three buttons, each behind a typed-name confirmation modal (per ui-elements.md §14):

- `Wipe drafts` — deletes everything under `~/.xvn/strategies/` (typed confirmation: `wipe drafts`)
- `Wipe runs + tape` — deletes `~/.xvn/runs/` and `~/.xvn/tape/` (typed: `wipe runs`)
- `Reset everything` — deletes `~/.xvn/` except `config/default.toml` and `secrets.env` (typed: `reset everything`)

Each handler should also emit a structured `tracing::warn!` so the user can find the action in logs.

- [ ] **Step 4: Tests for each handler**

```rust
#[tokio::test]
async fn wipe_drafts_removes_strategies_dir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("strategies")).unwrap();
    std::fs::write(dir.path().join("strategies/draft1.json"), "{}").unwrap();
    let app = build_router(AppState { xvn_home: dir.path().to_path_buf() });
    let resp = app.oneshot(
        Request::post("/api/settings/danger/wipe-drafts")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"confirm":"wipe drafts"}"#)).unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), 200);
    assert!(!dir.path().join("strategies/draft1.json").exists());
}

#[tokio::test]
async fn wipe_drafts_refuses_wrong_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_router(AppState { xvn_home: dir.path().to_path_buf() });
    let resp = app.oneshot(
        Request::post("/api/settings/danger/wipe-drafts")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"confirm":"yes"}"#)).unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), 400);
}
```

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(dashboard): /settings/{daemon,identity,danger} pages + danger-zone wipe handlers"
```

---

### Task 9: Placeholder pages for deferred sub-sections

**Files:**
- Modify: `crates/xianvec-dashboard/src/routes/settings.rs`

Five placeholders needed: `/settings/account`, `/settings/appearance`, `/settings/autoresearch`, `/settings/marketplace`, `/settings/telemetry`. Each renders `settings_base.html` with a body of one card explaining the deferral.

- [ ] **Step 1: Body copy**

| Route | Card title | Body |
|---|---|---|
| `/settings/account` | `Account` | `Single-workspace v1 — account management ships when multi-workspace lands.` |
| `/settings/appearance` | `Appearance` | `Dark theme only in v1. Theme pilot deferred — see `themes.md`.` |
| `/settings/autoresearch` | `Autoresearch` | `Configures the evening cycle (ε, hold-out window, parent policy, models). Ships with the autoresearcher plan series — see `docs/superpowers/specs/2026-05-09-karpathy-autoresearcher-design.md`.` |
| `/settings/marketplace` | `Marketplace` | `On-chain reputation via ERC-8004 on Mantle. Ships with Plan 5 (blockchain integration) — see `docs/superpowers/specs/2026-05-09-non-custodial-agent-wallets-design.md`.` |
| `/settings/telemetry` | `Telemetry` | `Tracing + token-usage rollups ship with the eval engine plan.` |

- [ ] **Step 2: Single shared handler**

```rust
pub async fn placeholder(section: &'static str, title: &'static str, body: &'static str) -> Response {
    PlaceholderPage { section, section_title: title, section_subtitle: None, body }.into_response()
}
```

- [ ] **Step 3: Commit**

```bash
git commit -am "feat(dashboard): placeholder cards for deferred /settings sub-sections"
```

---

## Phase E — Wiring + smoke

### Task 10: Provider chip on top nav reflects real state

**Files:**
- Modify: `crates/xianvec-dashboard/templates/base.html` (replace static pill with dynamic include)
- Modify: `crates/xianvec-dashboard/src/routes/api.rs` (add `GET /api/llm-status`)

The top nav (per ui-elements.md §1.2) shows `● Anthropic` / `● OpenAI` / `● No key` based on which provider's key is set. In Plan 2d this was a hardcoded chip; this task makes it reflect reality.

- [ ] **Step 1: Status endpoint**

```rust
pub async fn llm_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let cfg = xianvec_core::config::RuntimeConfig::load(
        &xianvec_core::config::config_path(&state.xvn_home)
    ).unwrap_or_default();
    let active = cfg.providers.iter().find(|p|
        !p.api_key_env.is_empty() && std::env::var(&p.api_key_env).is_ok()
    );
    Json(serde_json::json!({"name": active.map(|p| p.name.clone()), "kind": active.map(|p| p.kind.as_str())}))
}
```

- [ ] **Step 2: base.html injects status pill from JS**

Replace the static `● Anthropic` chip with `<span id="llm-status-pill" class="pill">…</span>` and add a small inline script to `base.html` that fetches `/api/llm-status` and updates the pill on every page load.

- [ ] **Step 3: Commit**

```bash
git commit -am "feat(dashboard): top-nav LLM pill reflects active provider state"
```

---

### Task 11: End-to-end smoke

Manual smoke (hackathon-acceptable per Plan 2d Task 11):

```bash
export XVN_HOME=/tmp/xvn-settings-smoke
mkdir -p $XVN_HOME/config
echo '[runtime]
mode = "backtest"' > $XVN_HOME/config/default.toml
xvn &
DASHBOARD_PID=$!
sleep 2
# Open localhost:7878 — should redirect to /setup (no key set)
# Paste an Anthropic key → submit → redirected to /. Wizard greets.
# Open Settings → Providers — anthropic row shows ● set
# Add an OpenAI provider with OPENAI_API_KEY (env unset) → row shows ○ missing
# Click Test on the openai row → red ✗ (TCP unreachable as no key)
# Try to delete the anthropic row → 409 (referenced by [intern])
# Visit /settings/danger → confirm-modal flow on Wipe drafts
kill $DASHBOARD_PID
```

Document the smoke procedure in `crates/xianvec-dashboard/README.md` under a "Settings & Onboarding" section.

Commit `chore: settings & onboarding smoke verified`.

---

### Task 12: Final workspace check

`cargo test --workspace` clean. clippy clean. fmt scoped to plan-touched crates. ~12 commits since this plan started.

---

## Self-review checklist

**Spec coverage:**
- [x] ui-elements.md §13.1 LLM keys → §13.1 Providers — Tasks 3–5
- [x] ui-elements.md §13.2 Brokers — Task 7
- [x] ui-elements.md §13.3 Daemon & runtime — Task 8
- [x] ui-elements.md §13.4 Identity (ERC-8004) — Task 8 (read-only stub)
- [x] ui-elements.md §13.5 Danger zone — Task 8
- [x] ui-elements.md §3.3 First-run / no-LLM-key state — Task 6
- [x] ui-elements.md §1.2 LLM status pill — Task 10
- [x] gptprompts.md §17 Settings · Autoresearch — Task 9 placeholder (defers to autoresearch plan)
- [x] gptprompts.md §18 Settings · Marketplace — Task 9 placeholder (defers to Plan 5)
- [x] LLM-providers spec §7.1 — Tasks 3–5 implement the design lock

**Out of scope as planned:**
- [ ] Autoresearch sub-page form — picked up by autoresearcher plan
- [ ] Marketplace wallet-connect flow — Plan 5
- [ ] Live identity mint/edit — Plan 5
- [ ] Light theme toggle — defer with theme pilot

**Type consistency:** `ProviderEntry`, `BrokerEntry`, `AppState`, all askama template structs, JS event handlers — consistent.

**Frequent commits:** 12 tasks → ~12 commits.

---

## What's next

Once this ships, `/settings/providers` is the canonical surface for the LLM-providers spec's UI design lock. Plan 5 (blockchain integration) picks up the marketplace + identity sub-pages. The autoresearcher plan series fills in `/settings/autoresearch`. Theme pilot remains deferred (intentionally).
