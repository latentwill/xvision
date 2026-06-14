//! Read-side export surface for the canonical agent-run ledger.
//!
//! This module loads a single `agent_runs` row plus its dependent
//! span/checkpoint/model_call/tool_call/approval/sandbox/note/artifact/event
//! rows from an open SQLite pool, and shapes them into two stable
//! deliverables:
//!
//! - [`AgentRunExport`] — serializes to the `xvn.agent_run.v2` JSON
//!   schema. Schema-version discipline is load-bearing here: a future
//!   shape change must bump `schema_version` rather than mutate v1 in
//!   place (see plan risk #5).
//! - [`AgentRunReport`] — serializes to plain-text Markdown. The header
//!   always carries a `Retention: <mode>` line so reports never imply
//!   more retention than was on. `--retention full_debug` runs surface
//!   a top-of-file warning banner.
//!
//! The export is **read-only**: it consumes the [`crate::SqliteRecorder`]
//! pool (or a parallel read-only handle on the same DB file) and never
//! mutates rows. It deliberately reuses the same row types declared in
//! `crate::rows` so the JSON shape stays in lock-step with what the
//! recorder writes.
//!
//! Top-level keys, per the observability plan:
//! `schema_version`, `run_id`, `objective`, `strategy_id`,
//! `eval_run_id`, `status`, `retention_mode`, `started_at`,
//! `finished_at`, `otel_trace_id`, `totals`, `spans` (recursive tree),
//! `model_calls`, `tool_calls`, `approvals`, `sandbox_results`,
//! `supervisor_notes`, `final_artifact`, plus the IPC-emission
//! additions `sidecar_version`, `cline_sdk_version`, `protocol_version`,
//! `mcp_servers`, `skills`.

use std::collections::HashMap;
use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};
use thiserror::Error;

use crate::blobs::{BlobRef, BlobStore};
use crate::rows::{
    AgentRunRow, ApprovalRow, ModelCallRow, SandboxResultRow, SpanRow, SupervisorNoteRow, ToolCallRow,
};

/// Schema-version tag stamped onto every export.
///
/// - v2 preserved the v1 top-level fields and added `accounting`
///   provenance for eval-linked runs.
/// - v3 (WS-7, "the flywheel document") adds the `events` array — every
///   `events` row for the run, not just `model_call_payload` — and inlines
///   blob-backed model/tool payloads so the export is self-contained.
///
/// Schema-version discipline is load-bearing: a shape change bumps this
/// tag rather than mutating an older version in place, so v1/v2 consumers
/// can detect the new shape instead of silently breaking.
pub const SCHEMA_VERSION: &str = "xvn.agent_run.v3";

/// Errors raised when building an export from the ledger.
#[derive(Debug, Error)]
pub enum ExportError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] sqlx::Error),
    #[error("agent run not found: {0}")]
    NotFound(String),
    #[error("invalid timestamp `{value}`: {source}")]
    InvalidTimestamp {
        value: String,
        #[source]
        source: chrono::ParseError,
    },
    #[error("invalid json blob in column `{column}`: {source}")]
    InvalidJson {
        column: &'static str,
        #[source]
        source: serde_json::Error,
    },
}

/// Aggregate counters surfaced at the top of the export. Computed from
/// the loaded detail rows (not cached in the agent_runs table).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExportTotals {
    pub model_calls: u64,
    pub tool_calls: u64,
    pub approvals: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
}

/// Token/accounting provenance for the top-level totals. Kept separate
/// from `model_calls` so exports can explain eval-level accounting even
/// when sidecar detail rows were not captured.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportAccounting {
    pub source: String,
    pub eval_run_id: Option<String>,
    pub eval_mode: Option<String>,
    pub eval_status: Option<String>,
    pub eval_actual_input_tokens: Option<u64>,
    pub eval_actual_output_tokens: Option<u64>,
    pub eval_model_calls: u64,
    pub eval_model_call_input_tokens: Option<u64>,
    pub eval_model_call_output_tokens: Option<u64>,
    pub eval_model_call_cost_usd: Option<f64>,
}

impl Default for ExportAccounting {
    fn default() -> Self {
        Self {
            source: "none".to_string(),
            eval_run_id: None,
            eval_mode: None,
            eval_status: None,
            eval_actual_input_tokens: None,
            eval_actual_output_tokens: None,
            eval_model_calls: 0,
            eval_model_call_input_tokens: None,
            eval_model_call_output_tokens: None,
            eval_model_call_cost_usd: None,
        }
    }
}

/// Recursive span tree node. Children are ordered by `started_at` then
/// by `id` (lexicographic) for deterministic output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanNode {
    #[serde(flatten)]
    pub row: SpanRow,
    pub children: Vec<SpanNode>,
}

/// Final artifact payload, serialized inline at `final_artifact` so
/// downstream consumers don't have to dereference through `artifact_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalArtifact {
    pub id: String,
    pub kind: String,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub hypothesis: Option<String>,
    pub recommendation: Option<String>,
    /// Already-parsed `[{label, value, source_span_id}]` if the JSON
    /// blob in the artifact row was valid; otherwise the raw string.
    pub evidence: Option<serde_json::Value>,
    pub next_experiments: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

/// One row from the `events` table, shaped for the export. Carries every
/// `EngineEvent` kind — `decision_completed`, `risk_veto`,
/// `regime_transition`, `filter_fired`, `order_state`,
/// `venue_account_snapshot`, `position_exit`, `memory_recall`,
/// `memory_write`, `model_call_payload`, `tool_call_payload`, … — so the
/// flywheel document is a complete record of what the run did, in order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportEvent {
    /// Span this event was scoped to, if any. `None` for run-scoped
    /// events (e.g. a filter firing not bracketed by a specific span).
    pub span_id: Option<String>,
    /// Producer-defined kind string (the `events.kind` column).
    pub kind: String,
    /// Parsed structured payload if `payload_json` was valid JSON;
    /// otherwise the raw string wrapped as a JSON string. `None` when the
    /// row carried no payload.
    pub payload_json: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

/// The `xvn.agent_run.v3` payload. Top-level field order follows the
/// schema layout in the plan so the serialized JSON is human-scannable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunExport {
    pub schema_version: &'static str,
    pub run_id: String,
    pub objective: String,
    pub strategy_id: Option<String>,
    pub eval_run_id: Option<String>,
    pub status: String,
    pub retention_mode: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub otel_trace_id: Option<String>,
    pub totals: ExportTotals,
    pub accounting: ExportAccounting,
    pub spans: Vec<SpanNode>,
    pub model_calls: Vec<ModelCallRow>,
    pub tool_calls: Vec<ToolCallRow>,
    pub approvals: Vec<ApprovalRow>,
    pub sandbox_results: Vec<SandboxResultRow>,
    pub supervisor_notes: Vec<SupervisorNoteRow>,
    /// Every `events` row for the run, in timeline (`created_at`, then
    /// `id`) order. WS-7: the headline full-fidelity addition — the old
    /// export loaded only the `model_call_payload` event as a correlated
    /// subquery and dropped every other engine/decision/risk/filter/
    /// order/regime/memory event on the floor.
    #[serde(default)]
    pub events: Vec<ExportEvent>,
    pub final_artifact: Option<FinalArtifact>,
    pub sidecar_version: Option<String>,
    pub cline_sdk_version: Option<String>,
    pub protocol_version: Option<String>,
    /// Run-time MCP server snapshot. Parsed from `mcp_servers_json` if
    /// it's valid JSON; otherwise serialized as `null`.
    pub mcp_servers: Option<serde_json::Value>,
    /// Run-time skills snapshot. Same parsing rule as `mcp_servers`.
    pub skills: Option<serde_json::Value>,
}

/// Markdown report companion to [`AgentRunExport`]. Implements
/// [`std::fmt::Display`] so callers can write `format!("{report}")`,
/// and serializes (via `serde`) as `{ "markdown": "<rendered>" }` for
/// API responses that want a structured envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunReport {
    pub markdown: String,
}

impl std::fmt::Display for AgentRunReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.markdown)
    }
}

// ─── Loaders ────────────────────────────────────────────────────────────────

/// Build a full [`AgentRunExport`] for `run_id` from the given pool.
///
/// Fans out to per-table loaders that share the pool. Each query runs
/// in its own statement against the same pool so this stays safe to
/// call concurrently with the recorder (write-side). If the run is not
/// found this returns [`ExportError::NotFound`].
pub async fn build_export(pool: &SqlitePool, run_id: &str) -> Result<AgentRunExport, ExportError> {
    build_export_with_blobs(pool, run_id, None).await
}

/// Build a full [`AgentRunExport`], optionally inlining blob-backed
/// payloads so the document is self-contained.
///
/// When `blobs` is `Some`, model-call prompt/response bodies and tool-call
/// input/output bodies are read from the content-addressed blob store
/// (using the refs stored on each row) and embedded into the export — so
/// a downstream coding agent can read the whole run without any follow-up
/// `/blobs/:ref` calls. When `blobs` is `None` (the default
/// [`build_export`] path), payloads still inline from the
/// `model_call_payload` / `tool_call_payload` event rows if present, but
/// raw blob refs are not dereferenced.
///
/// Blob inlining is best-effort: a missing or unreadable blob leaves the
/// corresponding `*_text` field as whatever the event-row reconstruction
/// produced (often `None`) rather than failing the whole export.
pub async fn build_export_with_blobs(
    pool: &SqlitePool,
    run_id: &str,
    blobs: Option<&BlobStore>,
) -> Result<AgentRunExport, ExportError> {
    let run = load_agent_run_or_eval_projection(pool, run_id).await?;
    let span_rows = load_spans(pool, run_id).await?;
    let mut model_calls = load_model_calls(pool, run_id).await?;
    let mut tool_calls = load_tool_calls(pool, run_id).await?;
    let approvals = load_approvals(pool, run_id).await?;
    let sandbox_results = load_sandbox_results(pool, run_id).await?;
    let supervisor_notes = load_supervisor_notes(pool, run_id).await?;
    let events = load_events(pool, run_id).await?;
    let final_artifact = if let Some(ref aid) = run.final_artifact_id {
        load_artifact(pool, aid).await?
    } else {
        None
    };

    if let Some(store) = blobs {
        inline_model_call_blobs(&mut model_calls, store);
        inline_tool_call_blobs(&mut tool_calls, store);
    }

    let detail_totals = compute_totals(&model_calls, &tool_calls, &approvals);
    let eval_accounting = load_eval_accounting(pool, &run.id, run.eval_run_id.as_deref()).await?;
    let (status, finished_at) = reconcile_status(&run, eval_accounting.as_ref());
    let (totals, accounting) = reconcile_totals(detail_totals, eval_accounting, &model_calls);
    let spans = into_tree(span_rows);

    let mcp_servers = parse_optional_json(run.mcp_servers_json.as_deref(), "mcp_servers_json")?;
    let skills = parse_optional_json(run.skills_json.as_deref(), "skills_json")?;

    Ok(AgentRunExport {
        schema_version: SCHEMA_VERSION,
        run_id: run.id,
        objective: run.objective,
        strategy_id: run.strategy_id,
        eval_run_id: run.eval_run_id,
        status,
        retention_mode: run.retention_mode,
        started_at: run.started_at,
        finished_at,
        otel_trace_id: run.otel_trace_id,
        totals,
        accounting,
        spans,
        model_calls,
        tool_calls,
        approvals,
        sandbox_results,
        supervisor_notes,
        events,
        final_artifact,
        sidecar_version: run.sidecar_version,
        cline_sdk_version: run.cline_sdk_version,
        protocol_version: run.protocol_version,
        mcp_servers,
        skills,
    })
}

/// Inline prompt/response bodies from the blob store onto each model call,
/// when the row's `*_payload_ref` resolves and the text wasn't already
/// reconstructed from the `model_call_payload` event.
fn inline_model_call_blobs(model_calls: &mut [ModelCallRow], store: &BlobStore) {
    for mc in model_calls.iter_mut() {
        if mc.prompt_text.is_none() {
            mc.prompt_text = read_blob_text(store, mc.prompt_payload_ref.as_deref());
        }
        if mc.response_text.is_none() {
            mc.response_text = read_blob_text(store, mc.response_payload_ref.as_deref());
        }
    }
}

/// Inline input/output bodies from the blob store onto each tool call.
fn inline_tool_call_blobs(tool_calls: &mut [ToolCallRow], store: &BlobStore) {
    for tc in tool_calls.iter_mut() {
        if tc.input_text.is_none() {
            tc.input_text = read_blob_text(store, tc.input_payload_ref.as_deref());
        }
        if tc.output_text.is_none() {
            tc.output_text = read_blob_text(store, tc.output_payload_ref.as_deref());
        }
    }
}

/// Read a blob ref into a UTF-8 string, lossily. Returns `None` for a
/// missing ref, a missing/unreadable blob, so blob inlining never fails
/// the whole export.
fn read_blob_text(store: &BlobStore, blob_ref: Option<&str>) -> Option<String> {
    let blob_ref = blob_ref?;
    let bytes = store.read(&BlobRef(blob_ref.to_owned())).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Build a markdown [`AgentRunReport`] for the same run. Idempotent on
/// terminal runs — produces identical bytes when called repeatedly.
pub async fn build_report(pool: &SqlitePool, run_id: &str) -> Result<AgentRunReport, ExportError> {
    let export = build_export(pool, run_id).await?;
    Ok(render_report(&export))
}

/// Render an already-loaded export as Markdown. Public so callers that
/// already have the JSON shape don't have to re-query SQLite.
pub fn render_report(export: &AgentRunExport) -> AgentRunReport {
    let mut out = String::new();

    if export.retention_mode == "full_debug" {
        let _ = writeln!(
            out,
            "> ⚠️  WARNING: retention mode is `full_debug` — this report \
             may contain prompts, tool inputs/outputs, and other sensitive \
             payloads. Treat as confidential."
        );
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "# Agent Run `{}`", export.run_id);
    let _ = writeln!(out);
    let _ = writeln!(out, "- Objective: {}", export.objective);
    let _ = writeln!(out, "- Status: {}", export.status);
    let _ = writeln!(out, "- Retention: {}", export.retention_mode);
    let _ = writeln!(out, "- Schema: {}", export.schema_version);
    let _ = writeln!(out, "- Fidelity: {}", fidelity_banner(export));
    if let Some(ref sid) = export.strategy_id {
        let _ = writeln!(out, "- Strategy: {sid}");
    }
    if let Some(ref eid) = export.eval_run_id {
        let _ = writeln!(out, "- Eval run: {eid}");
    }
    let _ = writeln!(out, "- Accounting source: {}", export.accounting.source);
    if let Some(ref mode) = export.accounting.eval_mode {
        let _ = writeln!(out, "- Eval mode: {mode}");
    }
    if let Some(ref status) = export.accounting.eval_status {
        let _ = writeln!(out, "- Eval status: {status}");
    }
    let _ = writeln!(
        out,
        "- Started at: {}",
        export
            .started_at
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    );
    if let Some(finished) = export.finished_at {
        let _ = writeln!(
            out,
            "- Finished at: {}",
            finished.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        );
    }
    if let Some(ref trace) = export.otel_trace_id {
        let _ = writeln!(out, "- OTel trace: {trace}");
    }
    let _ = writeln!(out);

    // Totals.
    let _ = writeln!(out, "## Totals");
    let _ = writeln!(out);
    let _ = writeln!(out, "| Metric | Value |");
    let _ = writeln!(out, "|---|---|");
    let _ = writeln!(out, "| Model calls | {} |", export.totals.model_calls);
    let _ = writeln!(out, "| Tool calls | {} |", export.totals.tool_calls);
    let _ = writeln!(out, "| Approvals | {} |", export.totals.approvals);
    let _ = writeln!(out, "| Input tokens | {} |", export.totals.input_tokens);
    let _ = writeln!(out, "| Output tokens | {} |", export.totals.output_tokens);
    let _ = writeln!(out, "| Cost USD | {:.6} |", export.totals.cost_usd);
    let _ = writeln!(out);

    // Span tree — the structural skeleton of the run, rendered as a
    // nested bullet list so a reader can see the shape at a glance.
    if !export.spans.is_empty() {
        let _ = writeln!(out, "## Span tree");
        let _ = writeln!(out);
        for root in &export.spans {
            render_span_node(&mut out, root, 0);
        }
        let _ = writeln!(out);
    }

    // Model calls / decisions — the full prompt + response for each model
    // call, inlined. This is the substance a coding agent needs.
    if !export.model_calls.is_empty() {
        let _ = writeln!(out, "## Model calls");
        let _ = writeln!(out);
        for (i, mc) in export.model_calls.iter().enumerate() {
            let _ = writeln!(
                out,
                "### Model call {} — {} / {} (span `{}`)",
                i + 1,
                mc.provider,
                mc.model,
                mc.span_id
            );
            let _ = writeln!(out);
            if let Some(reqs) = mc.tool_calls_requested.as_deref() {
                let _ = writeln!(out, "- Tool calls requested: `{reqs}`");
            }
            if let Some(cap) = mc.capability_path.as_deref() {
                let _ = writeln!(out, "- Capability path: `{cap}`");
            }
            let _ = writeln!(out);
            match mc.prompt_text.as_deref() {
                Some(p) => {
                    let _ = writeln!(out, "**Prompt.**");
                    let _ = writeln!(out);
                    render_fenced(&mut out, p);
                }
                None => {
                    let _ = writeln!(out, "**Prompt.** _(not retained — hash `{}`)_", mc.prompt_hash);
                    let _ = writeln!(out);
                }
            }
            match mc.response_text.as_deref() {
                Some(r) => {
                    let _ = writeln!(out, "**Response.**");
                    let _ = writeln!(out);
                    render_fenced(&mut out, r);
                }
                None => {
                    let hash = mc.response_hash.as_deref().unwrap_or("—");
                    let _ = writeln!(out, "**Response.** _(not retained — hash `{hash}`)_");
                    let _ = writeln!(out);
                }
            }
        }
    }

    // Tool calls — table summary plus inlined input/output bodies.
    if !export.tool_calls.is_empty() {
        let _ = writeln!(out, "## Tool calls");
        let _ = writeln!(out);
        let _ = writeln!(out, "| Tool | Origin | Risk | Exit |");
        let _ = writeln!(out, "|---|---|---|---|");
        for tc in &export.tool_calls {
            let exit = tc
                .exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "—".to_string());
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} |",
                tc.tool_name, tc.origin, tc.risk_level, exit
            );
        }
        let _ = writeln!(out);

        for tc in &export.tool_calls {
            if tc.input_text.is_none() && tc.output_text.is_none() {
                continue;
            }
            let _ = writeln!(out, "### `{}` (span `{}`)", tc.tool_name, tc.span_id);
            let _ = writeln!(out);
            if let Some(input) = tc.input_text.as_deref() {
                let _ = writeln!(out, "**Input.**");
                let _ = writeln!(out);
                render_fenced(&mut out, input);
            }
            if let Some(output) = tc.output_text.as_deref() {
                let _ = writeln!(out, "**Output.**");
                let _ = writeln!(out);
                render_fenced(&mut out, output);
            }
        }
    }

    // Event timeline — every `events` row in order. This is the WS-7
    // full-fidelity core: decisions, risk gates, filter firings, regime
    // transitions, broker/order events, memory recalls/writes, … all in
    // chronological order, so the document is a complete record.
    if !export.events.is_empty() {
        let _ = writeln!(out, "## Event timeline");
        let _ = writeln!(out);
        for ev in &export.events {
            let when = ev.created_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            let span = ev
                .span_id
                .as_deref()
                .map(|s| format!(" (span `{s}`)"))
                .unwrap_or_default();
            let _ = writeln!(out, "- `{when}` **{kind}**{span}", kind = ev.kind);
            if let Some(payload) = &ev.payload_json {
                let rendered = serde_json::to_string(payload).unwrap_or_else(|_| payload.to_string());
                let _ = writeln!(out, "  - {rendered}");
            }
        }
        let _ = writeln!(out);
    }

    // Supervisor notes — pull into headings by severity for operator
    // scanning. Sorted by created_at within each bucket.
    if !export.supervisor_notes.is_empty() {
        let _ = writeln!(out, "## Supervisor notes");
        let _ = writeln!(out);
        for note in &export.supervisor_notes {
            let _ = writeln!(
                out,
                "- [{severity}] **{role}** ({when}): {content}",
                severity = note.severity,
                role = note.role,
                when = note.created_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                content = note.content,
            );
        }
        let _ = writeln!(out);
    }

    // Final artifact.
    if let Some(ref art) = export.final_artifact {
        let _ = writeln!(out, "## Final artifact");
        let _ = writeln!(out);
        if let Some(ref title) = art.title {
            let _ = writeln!(out, "### {title}");
            let _ = writeln!(out);
        }
        if let Some(ref summary) = art.summary {
            let _ = writeln!(out, "**Summary.** {summary}");
            let _ = writeln!(out);
        }
        if let Some(ref hyp) = art.hypothesis {
            let _ = writeln!(out, "**Hypothesis.** {hyp}");
            let _ = writeln!(out);
        }
        if let Some(ref rec) = art.recommendation {
            let _ = writeln!(out, "**Recommendation.** {rec}");
            let _ = writeln!(out);
        }
    }

    AgentRunReport { markdown: out }
}

/// One-line fidelity statement for the report header. Surfaces clearly
/// whether full payloads (prompts / responses / tool I/O) were retained,
/// so an operator pasting this into a coding agent knows up-front whether
/// the document is complete or hash-only.
fn fidelity_banner(export: &AgentRunExport) -> String {
    let has_payloads = export
        .model_calls
        .iter()
        .any(|m| m.prompt_text.is_some() || m.response_text.is_some())
        || export
            .tool_calls
            .iter()
            .any(|t| t.input_text.is_some() || t.output_text.is_some());
    match export.retention_mode.as_str() {
        "hash_only" => "hash_only — payloads were NOT retained; prompts, responses, and tool I/O \
             are unavailable (hashes only). This document is a structural record, not a \
             full transcript."
            .to_string(),
        _ if has_payloads => "full — prompts, responses, and tool I/O are inlined below.".to_string(),
        _ => format!(
            "{} — retention allowed payloads, but none were captured for this run \
             (the run may predate payload capture, or no model/tool bodies were stored).",
            export.retention_mode
        ),
    }
}

/// Render one span subtree as an indented bullet list.
fn render_span_node(out: &mut String, node: &SpanNode, depth: usize) {
    let indent = "  ".repeat(depth);
    let dur = node
        .row
        .duration_ms
        .map(|ms| format!(" — {ms}ms"))
        .unwrap_or_default();
    let _ = writeln!(
        out,
        "{indent}- `{kind}` {name} [{status}]{dur} (span `{id}`)",
        kind = node.row.kind,
        name = node.row.name,
        status = node.row.status,
        id = node.row.id,
    );
    for child in &node.children {
        render_span_node(out, child, depth + 1);
    }
}

/// Emit a fenced code block. Picks a fence long enough to not collide with
/// any backtick run inside the body, so payloads that themselves contain
/// triple backticks still render as one block.
fn render_fenced(out: &mut String, body: &str) {
    let longest_run = body.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest_run.max(2) + 1);
    let _ = writeln!(out, "{fence}");
    let _ = writeln!(out, "{}", body.trim_end_matches('\n'));
    let _ = writeln!(out, "{fence}");
    let _ = writeln!(out);
}

// ─── blob ownership lookup ──────────────────────────────────────────────────

/// Resolves `(run_id, blob_ref) → Option<retention_mode_db_str>`.
///
/// Returns the run's `retention_mode` as stored in `agent_runs`
/// (`hash_only` | `redacted` | `full_debug`) if `blob_ref` is
/// referenced by any `model_calls`, `tool_calls`, or `checkpoints`
/// row whose span (or row, for checkpoints) belongs to `run_id`.
/// Returns `Ok(None)` if no such row exists — the caller should map
/// that to 404.
///
/// The ref is matched as a literal string; this function does not
/// hash or normalize it. Callers must validate the ref's shape
/// (typically `^[0-9a-f]{64}$`) before invoking. The intent of the
/// shape check is to refuse traversal (`..`, `/`) before the blob
/// store joins the ref onto its root path.
pub async fn find_blob_owner(
    pool: &SqlitePool,
    run_id: &str,
    blob_ref: &str,
) -> Result<Option<String>, ExportError> {
    // One round-trip via three `EXISTS` subqueries. SQLite short-circuits
    // on the first match, so the worst case is the same as a single
    // indexed scan over the smallest detail table.
    let row: Option<SqliteRow> = sqlx::query(
        "SELECT ar.retention_mode FROM agent_runs ar \
         WHERE ar.id = ?1 AND ( \
            EXISTS ( \
                SELECT 1 FROM model_calls mc \
                JOIN spans s ON s.id = mc.span_id \
                WHERE s.run_id = ar.id \
                AND (mc.prompt_payload_ref = ?2 OR mc.response_payload_ref = ?2) \
            ) \
            OR EXISTS ( \
                SELECT 1 FROM tool_calls tc \
                JOIN spans s ON s.id = tc.span_id \
                WHERE s.run_id = ar.id \
                AND (tc.input_payload_ref = ?2 OR tc.output_payload_ref = ?2) \
            ) \
            OR EXISTS ( \
                SELECT 1 FROM checkpoints c \
                WHERE c.run_id = ar.id \
                AND (c.input_payload_ref = ?2 OR c.output_payload_ref = ?2) \
            ) \
         )",
    )
    .bind(run_id)
    .bind(blob_ref)
    .fetch_optional(pool)
    .await?;

    Ok(row
        .map(|r| r.try_get::<String, _>("retention_mode"))
        .transpose()?)
}

// ─── per-table loaders ──────────────────────────────────────────────────────

async fn load_agent_run(pool: &SqlitePool, run_id: &str) -> Result<AgentRunRow, ExportError> {
    let row: Option<SqliteRow> = sqlx::query(
        "SELECT id, objective, strategy_id, eval_run_id, source_cli_job_id, \
                status, started_at, finished_at, retention_mode, \
                sidecar_version, cline_sdk_version, protocol_version, \
                skills_json, mcp_servers_json, otel_trace_id, \
                final_artifact_id, error \
         FROM agent_runs WHERE id = ?",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or_else(|| ExportError::NotFound(run_id.to_owned()))?;
    let started_at: String = row.try_get("started_at")?;
    let finished_at: Option<String> = row.try_get("finished_at")?;
    Ok(AgentRunRow {
        id: row.try_get("id")?,
        objective: row.try_get("objective")?,
        strategy_id: row.try_get("strategy_id")?,
        eval_run_id: row.try_get("eval_run_id")?,
        source_cli_job_id: row.try_get("source_cli_job_id")?,
        status: row.try_get("status")?,
        started_at: parse_ts(&started_at)?,
        finished_at: finished_at.as_deref().map(parse_ts).transpose()?,
        retention_mode: row.try_get("retention_mode")?,
        sidecar_version: row.try_get("sidecar_version")?,
        cline_sdk_version: row.try_get("cline_sdk_version")?,
        protocol_version: row.try_get("protocol_version")?,
        skills_json: row.try_get("skills_json")?,
        mcp_servers_json: row.try_get("mcp_servers_json")?,
        otel_trace_id: row.try_get("otel_trace_id")?,
        final_artifact_id: row.try_get("final_artifact_id")?,
        error: row.try_get("error")?,
    })
}

async fn load_agent_run_or_eval_projection(
    pool: &SqlitePool,
    run_id: &str,
) -> Result<AgentRunRow, ExportError> {
    match load_agent_run(pool, run_id).await {
        Ok(run) => Ok(run),
        Err(ExportError::NotFound(_)) => load_eval_run_projection(pool, run_id).await,
        Err(e) => Err(e),
    }
}

async fn load_eval_run_projection(pool: &SqlitePool, eval_run_id: &str) -> Result<AgentRunRow, ExportError> {
    if !table_exists(pool, "eval_runs").await? {
        return Err(ExportError::NotFound(eval_run_id.to_owned()));
    }

    let row: Option<SqliteRow> = sqlx::query(
        "SELECT id, status, started_at, completed_at \
         FROM eval_runs WHERE id = ?",
    )
    .bind(eval_run_id)
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or_else(|| ExportError::NotFound(eval_run_id.to_owned()))?;
    let started_at: String = row.try_get("started_at")?;
    let finished_at: Option<String> = row.try_get("completed_at")?;
    Ok(AgentRunRow {
        id: row.try_get("id")?,
        objective: format!("Eval run {eval_run_id}"),
        strategy_id: None,
        eval_run_id: Some(eval_run_id.to_owned()),
        source_cli_job_id: None,
        status: row.try_get("status")?,
        started_at: parse_ts(&started_at)?,
        finished_at: finished_at.as_deref().map(parse_ts).transpose()?,
        retention_mode: "hash_only".to_string(),
        sidecar_version: None,
        cline_sdk_version: None,
        protocol_version: None,
        skills_json: None,
        mcp_servers_json: None,
        otel_trace_id: None,
        final_artifact_id: None,
        error: None,
    })
}

#[derive(Debug, Clone)]
struct EvalAccountingRow {
    eval_run_id: String,
    mode: String,
    status: String,
    completed_at: Option<DateTime<Utc>>,
    actual_input_tokens: Option<u64>,
    actual_output_tokens: Option<u64>,
    model_calls: u64,
    model_call_input_tokens: Option<u64>,
    model_call_output_tokens: Option<u64>,
    model_call_cost_usd: Option<f64>,
}

async fn load_eval_accounting(
    pool: &SqlitePool,
    agent_run_id: &str,
    linked_eval_run_id: Option<&str>,
) -> Result<Option<EvalAccountingRow>, ExportError> {
    if !table_exists(pool, "eval_runs").await? {
        return Ok(None);
    }

    let eval_run_id = if let Some(id) = linked_eval_run_id {
        Some(id.to_owned())
    } else {
        let direct: Option<String> = sqlx::query_scalar("SELECT id FROM eval_runs WHERE id = ?")
            .bind(agent_run_id)
            .fetch_optional(pool)
            .await?;
        direct
    };
    let Some(eval_run_id) = eval_run_id else {
        return Ok(None);
    };

    let row: Option<SqliteRow> = sqlx::query(
        "SELECT id, mode, status, completed_at, actual_input_tokens, actual_output_tokens \
         FROM eval_runs WHERE id = ?",
    )
    .bind(&eval_run_id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };
    let completed_at_raw: Option<String> = row.try_get("completed_at")?;
    let (model_calls, sum_in, sum_out, sum_cost) = load_eval_model_call_totals(pool, &eval_run_id).await?;

    Ok(Some(EvalAccountingRow {
        eval_run_id: row.try_get("id")?,
        mode: row.try_get("mode")?,
        status: row.try_get("status")?,
        completed_at: completed_at_raw.as_deref().map(parse_ts).transpose()?,
        actual_input_tokens: row
            .try_get::<Option<i64>, _>("actual_input_tokens")?
            .and_then(non_negative_u64),
        actual_output_tokens: row
            .try_get::<Option<i64>, _>("actual_output_tokens")?
            .and_then(non_negative_u64),
        model_calls,
        model_call_input_tokens: sum_in.and_then(non_negative_u64),
        model_call_output_tokens: sum_out.and_then(non_negative_u64),
        model_call_cost_usd: sum_cost,
    }))
}

async fn load_eval_model_call_totals(
    pool: &SqlitePool,
    eval_run_id: &str,
) -> Result<(u64, Option<i64>, Option<i64>, Option<f64>), ExportError> {
    if !table_exists(pool, "agent_runs").await?
        || !table_exists(pool, "spans").await?
        || !table_exists(pool, "model_calls").await?
    {
        return Ok((0, None, None, None));
    }

    let row: Option<SqliteRow> = sqlx::query(
        "SELECT COUNT(*) AS rows, \
                SUM(mc.input_token_count) AS sum_in, \
                SUM(mc.output_token_count) AS sum_out, \
                SUM(mc.cost_usd) AS sum_cost \
         FROM model_calls mc \
         JOIN spans s ON s.id = mc.span_id \
         JOIN agent_runs ar ON ar.id = s.run_id \
         WHERE ar.eval_run_id = ?",
    )
    .bind(eval_run_id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok((0, None, None, None));
    };
    let rows = row
        .try_get::<i64, _>("rows")
        .ok()
        .and_then(non_negative_u64)
        .unwrap_or(0);
    Ok((
        rows,
        row.try_get::<Option<i64>, _>("sum_in")?,
        row.try_get::<Option<i64>, _>("sum_out")?,
        row.try_get::<Option<f64>, _>("sum_cost")?,
    ))
}

fn reconcile_status(
    run: &AgentRunRow,
    accounting: Option<&EvalAccountingRow>,
) -> (String, Option<DateTime<Utc>>) {
    let Some(accounting) = accounting else {
        return (run.status.clone(), run.finished_at);
    };
    if is_terminal_eval_status(&accounting.status) && is_nonterminal_agent_status(&run.status) {
        return (
            accounting.status.clone(),
            accounting.completed_at.or(run.finished_at),
        );
    }
    (run.status.clone(), run.finished_at)
}

fn reconcile_totals(
    mut totals: ExportTotals,
    eval_accounting: Option<EvalAccountingRow>,
    model_calls: &[ModelCallRow],
) -> (ExportTotals, ExportAccounting) {
    let Some(eval) = eval_accounting else {
        let source = if model_calls.is_empty() {
            "none"
        } else {
            "agent_model_calls"
        };
        return (
            totals,
            ExportAccounting {
                source: source.to_string(),
                ..Default::default()
            },
        );
    };

    let mut accounting = ExportAccounting {
        source: "none".to_string(),
        eval_run_id: Some(eval.eval_run_id.clone()),
        eval_mode: Some(eval.mode),
        eval_status: Some(eval.status),
        eval_actual_input_tokens: eval.actual_input_tokens,
        eval_actual_output_tokens: eval.actual_output_tokens,
        eval_model_calls: eval.model_calls,
        eval_model_call_input_tokens: eval.model_call_input_tokens,
        eval_model_call_output_tokens: eval.model_call_output_tokens,
        eval_model_call_cost_usd: eval.model_call_cost_usd,
    };

    if eval.model_calls > 0 {
        totals.model_calls = eval.model_calls;
        totals.input_tokens = eval.model_call_input_tokens.unwrap_or_default();
        totals.output_tokens = eval.model_call_output_tokens.unwrap_or_default();
        totals.cost_usd = eval.model_call_cost_usd.unwrap_or_default();
        accounting.source = "eval_model_calls".to_string();
    } else if eval.actual_input_tokens.unwrap_or_default() > 0
        || eval.actual_output_tokens.unwrap_or_default() > 0
    {
        totals.input_tokens = eval.actual_input_tokens.unwrap_or_default();
        totals.output_tokens = eval.actual_output_tokens.unwrap_or_default();
        accounting.source = "eval_actuals".to_string();
    } else if !model_calls.is_empty() {
        accounting.source = "agent_model_calls".to_string();
    }

    (totals, accounting)
}

async fn table_exists(pool: &SqlitePool, table: &str) -> Result<bool, ExportError> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_one(pool)
            .await?;
    Ok(count > 0)
}

fn non_negative_u64(n: i64) -> Option<u64> {
    if n >= 0 {
        Some(n as u64)
    } else {
        None
    }
}

fn is_terminal_eval_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled")
}

fn is_nonterminal_agent_status(status: &str) -> bool {
    matches!(status, "queued" | "running")
}

async fn load_spans(pool: &SqlitePool, run_id: &str) -> Result<Vec<SpanRow>, ExportError> {
    // Ordered for deterministic golden output. `started_at` is the
    // primary key; ids tie-break when two spans share a timestamp.
    let rows: Vec<SqliteRow> = sqlx::query(
        "SELECT id, run_id, parent_span_id, otel_trace_id, otel_span_id, \
                kind, name, status, started_at, ended_at, duration_ms, \
                attributes_json, error_json \
         FROM spans WHERE run_id = ? \
         ORDER BY started_at ASC, id ASC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            let started_at: String = r.try_get("started_at")?;
            let ended_at: Option<String> = r.try_get("ended_at")?;
            Ok(SpanRow {
                id: r.try_get("id")?,
                run_id: r.try_get("run_id")?,
                parent_span_id: r.try_get("parent_span_id")?,
                otel_trace_id: r.try_get("otel_trace_id")?,
                otel_span_id: r.try_get("otel_span_id")?,
                kind: r.try_get("kind")?,
                name: r.try_get("name")?,
                status: r.try_get("status")?,
                started_at: parse_ts(&started_at)?,
                ended_at: ended_at.as_deref().map(parse_ts).transpose()?,
                duration_ms: r.try_get("duration_ms")?,
                attributes_json: r.try_get("attributes_json")?,
                error_json: r.try_get("error_json")?,
            })
        })
        .collect()
}

async fn load_model_calls(pool: &SqlitePool, run_id: &str) -> Result<Vec<ModelCallRow>, ExportError> {
    let rows: Vec<SqliteRow> = sqlx::query(
        "SELECT mc.span_id, mc.provider, mc.model, mc.input_token_count, \
                mc.output_token_count, mc.cost_usd, mc.prompt_hash, \
                mc.response_hash, mc.prompt_payload_ref, mc.response_payload_ref, \
                mc.tool_calls_requested, mc.capability_path, \
                (SELECT e.payload_json \
                   FROM events e \
                  WHERE e.span_id = mc.span_id AND e.kind = 'model_call_payload' \
                  ORDER BY e.created_at DESC, e.id DESC \
                  LIMIT 1) AS model_call_payload_json \
         FROM model_calls mc \
         JOIN spans s ON s.id = mc.span_id \
         WHERE s.run_id = ? \
         ORDER BY s.started_at ASC, mc.span_id ASC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            let payload_json: Option<String> = r.try_get("model_call_payload_json")?;
            let (prompt_text, response_text) = parse_model_call_payload(payload_json.as_deref());
            Ok(ModelCallRow {
                span_id: r.try_get("span_id")?,
                provider: r.try_get("provider")?,
                model: r.try_get("model")?,
                input_token_count: r.try_get("input_token_count")?,
                output_token_count: r.try_get("output_token_count")?,
                cost_usd: r.try_get("cost_usd")?,
                prompt_hash: r.try_get("prompt_hash")?,
                response_hash: r.try_get("response_hash")?,
                prompt_text,
                response_text,
                prompt_payload_ref: r.try_get("prompt_payload_ref")?,
                response_payload_ref: r.try_get("response_payload_ref")?,
                tool_calls_requested: r.try_get("tool_calls_requested")?,
                capability_path: r.try_get("capability_path")?,
            })
        })
        .collect()
}

fn parse_model_call_payload(payload_json: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(raw) = payload_json else {
        return (None, None);
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return (None, None);
    };
    let prompt = value
        .get("prompt")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned);
    let response = value
        .get("response")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned);
    (prompt, response)
}

/// Extract a single string field (`input` / `output`) from a
/// `tool_call_payload` side-row's `payload_json`. Mirrors
/// `parse_model_call_payload`, but tool input and output arrive on
/// separate events so each is reconstructed independently.
fn parse_tool_call_payload_field(payload_json: Option<&str>, field: &str) -> Option<String> {
    let raw = payload_json?;
    let value = serde_json::from_str::<serde_json::Value>(raw).ok()?;
    json_field_to_text(value.get(field))
}

async fn load_tool_calls(pool: &SqlitePool, run_id: &str) -> Result<Vec<ToolCallRow>, ExportError> {
    // The `tool_call_payload` correlated subquery mirrors the
    // `model_call_payload` reconstruction on `load_model_calls`: WS-5
    // added a `tool_call_payload` side-row carrying the tool's input /
    // output bodies, so the full-fidelity export can inline tool I/O
    // even when the blob store isn't available.
    let rows: Vec<SqliteRow> = sqlx::query(
        "SELECT tc.span_id, tc.tool_name, tc.origin, tc.tool_version, tc.tool_hash, \
                tc.input_hash, tc.output_hash, tc.input_payload_ref, tc.output_payload_ref, \
                tc.side_effect_level, tc.risk_level, tc.requires_approval, tc.approval_id, \
                tc.exit_code, tc.is_run_terminator, \
                (SELECT e.payload_json \
                   FROM events e \
                  WHERE e.span_id = tc.span_id AND e.kind = 'tool_call_payload' \
                    AND json_extract(e.payload_json, '$.input') IS NOT NULL \
                  ORDER BY e.created_at DESC, e.id DESC \
                  LIMIT 1) AS tool_input_payload_json, \
                (SELECT e.payload_json \
                   FROM events e \
                  WHERE e.span_id = tc.span_id AND e.kind = 'tool_call_payload' \
                    AND json_extract(e.payload_json, '$.output') IS NOT NULL \
                  ORDER BY e.created_at DESC, e.id DESC \
                  LIMIT 1) AS tool_output_payload_json \
         FROM tool_calls tc \
         JOIN spans s ON s.id = tc.span_id \
         WHERE s.run_id = ? \
         ORDER BY s.started_at ASC, tc.span_id ASC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            let requires_approval: i64 = r.try_get("requires_approval")?;
            let is_run_terminator: i64 = r.try_get("is_run_terminator")?;
            let input_payload_json: Option<String> = r.try_get("tool_input_payload_json")?;
            let output_payload_json: Option<String> = r.try_get("tool_output_payload_json")?;
            let input_text = parse_tool_call_payload_field(input_payload_json.as_deref(), "input");
            let output_text = parse_tool_call_payload_field(output_payload_json.as_deref(), "output");
            Ok(ToolCallRow {
                span_id: r.try_get("span_id")?,
                tool_name: r.try_get("tool_name")?,
                origin: r.try_get("origin")?,
                tool_version: r.try_get("tool_version")?,
                tool_hash: r.try_get("tool_hash")?,
                input_hash: r.try_get("input_hash")?,
                output_hash: r.try_get("output_hash")?,
                input_text,
                output_text,
                input_payload_ref: r.try_get("input_payload_ref")?,
                output_payload_ref: r.try_get("output_payload_ref")?,
                side_effect_level: r.try_get("side_effect_level")?,
                risk_level: r.try_get("risk_level")?,
                requires_approval: requires_approval != 0,
                approval_id: r.try_get("approval_id")?,
                exit_code: r.try_get("exit_code")?,
                is_run_terminator: is_run_terminator != 0,
            })
        })
        .collect()
}

/// Coerce a JSON field into display text: strings pass through; other
/// values are compactly re-serialized; `null`/absent → `None`.
fn json_field_to_text(value: Option<&serde_json::Value>) -> Option<String> {
    match value {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

async fn load_approvals(pool: &SqlitePool, run_id: &str) -> Result<Vec<ApprovalRow>, ExportError> {
    let rows: Vec<SqliteRow> = sqlx::query(
        "SELECT a.id, a.span_id, a.tool_call_id, a.reason, a.risk_level, \
                a.requested_at, a.decided_at, a.decision, a.decided_by \
         FROM approvals a \
         JOIN spans s ON s.id = a.span_id \
         WHERE s.run_id = ? \
         ORDER BY a.requested_at ASC, a.id ASC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            let requested_at: String = r.try_get("requested_at")?;
            let decided_at: Option<String> = r.try_get("decided_at")?;
            Ok(ApprovalRow {
                id: r.try_get("id")?,
                span_id: r.try_get("span_id")?,
                tool_call_id: r.try_get("tool_call_id")?,
                reason: r.try_get("reason")?,
                risk_level: r.try_get("risk_level")?,
                requested_at: parse_ts(&requested_at)?,
                decided_at: decided_at.as_deref().map(parse_ts).transpose()?,
                decision: r.try_get("decision")?,
                decided_by: r.try_get("decided_by")?,
            })
        })
        .collect()
}

async fn load_sandbox_results(pool: &SqlitePool, run_id: &str) -> Result<Vec<SandboxResultRow>, ExportError> {
    let rows: Vec<SqliteRow> = sqlx::query(
        "SELECT sr.span_id, sr.command, sr.cwd, sr.stdout_ref, sr.stderr_ref, \
                sr.exit_code, sr.duration_ms \
         FROM sandbox_results sr \
         JOIN spans s ON s.id = sr.span_id \
         WHERE s.run_id = ? \
         ORDER BY s.started_at ASC, sr.span_id ASC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            Ok(SandboxResultRow {
                span_id: r.try_get("span_id")?,
                command: r.try_get("command")?,
                cwd: r.try_get("cwd")?,
                stdout_ref: r.try_get("stdout_ref")?,
                stderr_ref: r.try_get("stderr_ref")?,
                exit_code: r.try_get("exit_code")?,
                duration_ms: r.try_get("duration_ms")?,
            })
        })
        .collect()
}

async fn load_supervisor_notes(
    pool: &SqlitePool,
    run_id: &str,
) -> Result<Vec<SupervisorNoteRow>, ExportError> {
    let rows: Vec<SqliteRow> = sqlx::query(
        "SELECT id, run_id, role, content, severity, created_at \
         FROM supervisor_notes \
         WHERE run_id = ? \
         ORDER BY created_at ASC, id ASC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            let created_at: String = r.try_get("created_at")?;
            Ok(SupervisorNoteRow {
                id: r.try_get("id")?,
                run_id: r.try_get("run_id")?,
                role: r.try_get("role")?,
                content: r.try_get("content")?,
                severity: r.try_get("severity")?,
                created_at: parse_ts(&created_at)?,
            })
        })
        .collect()
}

/// Load every `events` row for the run, in timeline order. WS-7: this is
/// the headline full-fidelity loader — the old export never read the
/// `events` table directly (it only pulled `model_call_payload` as a
/// correlated subquery off `model_calls`), so all the engine/decision/
/// risk/filter/order/regime/memory events were absent from the document.
async fn load_events(pool: &SqlitePool, run_id: &str) -> Result<Vec<ExportEvent>, ExportError> {
    let rows: Vec<SqliteRow> = sqlx::query(
        "SELECT span_id, kind, payload_json, created_at \
         FROM events WHERE run_id = ? \
         ORDER BY created_at ASC, id ASC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            let created_at: String = r.try_get("created_at")?;
            let payload_raw: Option<String> = r.try_get("payload_json")?;
            Ok(ExportEvent {
                span_id: r.try_get("span_id")?,
                kind: r.try_get("kind")?,
                payload_json: parse_event_payload(payload_raw.as_deref()),
                created_at: parse_ts(&created_at)?,
            })
        })
        .collect()
}

/// Parse an event payload into structured JSON. A row that is not valid
/// JSON is preserved verbatim as a JSON string rather than dropped — the
/// flywheel document must not silently lose payloads. An empty payload
/// maps to `None`.
fn parse_event_payload(raw: Option<&str>) -> Option<serde_json::Value> {
    let raw = raw?;
    if raw.is_empty() {
        return None;
    }
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(value) => Some(canonical_json(value)),
        Err(_) => Some(serde_json::Value::String(raw.to_owned())),
    }
}

async fn load_artifact(pool: &SqlitePool, artifact_id: &str) -> Result<Option<FinalArtifact>, ExportError> {
    let row: Option<SqliteRow> = sqlx::query(
        "SELECT id, run_id, kind, title, summary, hypothesis, recommendation, \
                evidence_json, next_experiments_json, created_at \
         FROM artifacts WHERE id = ?",
    )
    .bind(artifact_id)
    .fetch_optional(pool)
    .await?;

    let Some(r) = row else {
        return Ok(None);
    };

    let evidence_json: Option<String> = r.try_get("evidence_json")?;
    let next_experiments_json: Option<String> = r.try_get("next_experiments_json")?;
    let created_at: String = r.try_get("created_at")?;
    let evidence = parse_optional_json(evidence_json.as_deref(), "artifacts.evidence_json")?;
    let next_experiments = parse_optional_json(
        next_experiments_json.as_deref(),
        "artifacts.next_experiments_json",
    )?;

    Ok(Some(FinalArtifact {
        id: r.try_get("id")?,
        kind: r.try_get("kind")?,
        title: r.try_get("title")?,
        summary: r.try_get("summary")?,
        hypothesis: r.try_get("hypothesis")?,
        recommendation: r.try_get("recommendation")?,
        evidence,
        next_experiments,
        created_at: parse_ts(&created_at)?,
    }))
}

// ─── helpers ────────────────────────────────────────────────────────────────

fn parse_ts(s: &str) -> Result<DateTime<Utc>, ExportError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|source| ExportError::InvalidTimestamp {
            value: s.to_owned(),
            source,
        })
}

fn parse_optional_json(
    raw: Option<&str>,
    column: &'static str,
) -> Result<Option<serde_json::Value>, ExportError> {
    let Some(raw) = raw else { return Ok(None) };
    if raw.is_empty() {
        return Ok(None);
    }
    serde_json::from_str(raw)
        .map(|value| Some(canonical_json(value)))
        .map_err(|source| ExportError::InvalidJson { column, source })
}

fn canonical_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(canonical_json).collect())
        }
        serde_json::Value::Object(map) => {
            let mut entries: Vec<_> = map.into_iter().collect();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            let mut sorted = serde_json::Map::new();
            for (key, value) in entries {
                sorted.insert(key, canonical_json(value));
            }
            serde_json::Value::Object(sorted)
        }
        value => value,
    }
}

fn compute_totals(
    model_calls: &[ModelCallRow],
    tool_calls: &[ToolCallRow],
    approvals: &[ApprovalRow],
) -> ExportTotals {
    let mut totals = ExportTotals {
        model_calls: model_calls.len() as u64,
        tool_calls: tool_calls.len() as u64,
        approvals: approvals.len() as u64,
        ..Default::default()
    };
    for mc in model_calls {
        if let Some(n) = mc.input_token_count {
            if n > 0 {
                totals.input_tokens = totals.input_tokens.saturating_add(n as u64);
            }
        }
        if let Some(n) = mc.output_token_count {
            if n > 0 {
                totals.output_tokens = totals.output_tokens.saturating_add(n as u64);
            }
        }
        if let Some(c) = mc.cost_usd {
            totals.cost_usd += c;
        }
    }
    totals
}

/// Build a parent → children tree from a flat span list. Spans whose
/// `parent_span_id` is unknown (orphans) are surfaced at the root so
/// nothing is silently lost.
fn into_tree(rows: Vec<SpanRow>) -> Vec<SpanNode> {
    // Preserve input order for deterministic output. We use a HashMap
    // for the parent lookup but iterate the original Vec when shaping.
    let id_set: std::collections::HashSet<String> = rows.iter().map(|r| r.id.clone()).collect();

    let mut children_of: HashMap<String, Vec<SpanNode>> = HashMap::new();
    let mut roots: Vec<SpanNode> = Vec::new();

    // Walk in reverse so children land before their parents in the
    // intermediate map; when we pop parents we already have their
    // children list ready.
    let mut leaves_first: Vec<SpanRow> = rows;
    leaves_first.sort_by(|a, b| b.started_at.cmp(&a.started_at).then_with(|| b.id.cmp(&a.id)));

    for row in leaves_first {
        let node = SpanNode {
            children: children_of.remove(&row.id).unwrap_or_default(),
            row,
        };
        match &node.row.parent_span_id {
            Some(parent_id) if id_set.contains(parent_id) => {
                children_of.entry(parent_id.clone()).or_default().push(node);
            }
            _ => roots.push(node),
        }
    }

    // We pushed roots in reverse order (largest started_at first); flip
    // and re-sort each child list back into started_at ASC.
    roots.sort_by(|a, b| {
        a.row
            .started_at
            .cmp(&b.row.started_at)
            .then_with(|| a.row.id.cmp(&b.row.id))
    });
    for root in &mut roots {
        sort_children(root);
    }
    roots
}

fn sort_children(node: &mut SpanNode) {
    node.children.sort_by(|a, b| {
        a.row
            .started_at
            .cmp(&b.row.started_at)
            .then_with(|| a.row.id.cmp(&b.row.id))
    });
    for child in &mut node.children {
        sort_children(child);
    }
}

#[cfg(test)]
mod blob_owner_tests {
    use super::*;
    use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

    const MIGRATION_002: &str = include_str!("../../xvision-engine/migrations/002_eval.sql");
    const MIGRATION_013: &str = include_str!("../../xvision-engine/migrations/013_cli_jobs.sql");
    const MIGRATION_018: &str =
        include_str!("../../xvision-engine/migrations/018_agent_run_observability.sql");

    async fn migrated_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
        sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
        sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
        pool
    }

    async fn seed_run(pool: &SqlitePool, run_id: &str, retention: &str) {
        sqlx::query(
            "INSERT INTO agent_runs (id, objective, status, started_at, retention_mode) \
             VALUES (?1, 'test', 'completed', '2026-05-17T16:00:00Z', ?2)",
        )
        .bind(run_id)
        .bind(retention)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_span(pool: &SqlitePool, span_id: &str, run_id: &str) {
        sqlx::query(
            "INSERT INTO spans (id, run_id, kind, name, status, started_at) \
             VALUES (?1, ?2, 'decision.model', 'm', 'ok', '2026-05-17T16:00:01Z')",
        )
        .bind(span_id)
        .bind(run_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_model_call(
        pool: &SqlitePool,
        span_id: &str,
        prompt_ref: Option<&str>,
        response_ref: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO model_calls (span_id, provider, model, prompt_hash, \
                 prompt_payload_ref, response_payload_ref) \
             VALUES (?1, 'anthropic', 'claude', 'sha256:abc', ?2, ?3)",
        )
        .bind(span_id)
        .bind(prompt_ref)
        .bind(response_ref)
        .execute(pool)
        .await
        .unwrap();
    }

    const REF_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const REF_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const REF_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    const REF_NOT_OWNED: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

    #[tokio::test]
    async fn returns_retention_mode_when_model_call_owns_prompt_ref() {
        let pool = migrated_pool().await;
        seed_run(&pool, "run_x", "full_debug").await;
        seed_span(&pool, "span_m1", "run_x").await;
        seed_model_call(&pool, "span_m1", Some(REF_A), Some(REF_B)).await;

        let got = find_blob_owner(&pool, "run_x", REF_A).await.unwrap();
        assert_eq!(got.as_deref(), Some("full_debug"));

        // Response ref on the same row also resolves.
        let got = find_blob_owner(&pool, "run_x", REF_B).await.unwrap();
        assert_eq!(got.as_deref(), Some("full_debug"));
    }

    #[tokio::test]
    async fn returns_none_when_ref_not_owned_by_this_run() {
        let pool = migrated_pool().await;
        seed_run(&pool, "run_x", "redacted").await;
        seed_span(&pool, "span_m1", "run_x").await;
        seed_model_call(&pool, "span_m1", Some(REF_A), None).await;

        // Wrong ref → None.
        let got = find_blob_owner(&pool, "run_x", REF_NOT_OWNED).await.unwrap();
        assert!(got.is_none());

        // Right ref but wrong run → None.
        let got = find_blob_owner(&pool, "run_other", REF_A).await.unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn checkpoint_refs_also_resolve() {
        let pool = migrated_pool().await;
        seed_run(&pool, "run_cp", "full_debug").await;
        seed_span(&pool, "span_cp", "run_cp").await;
        sqlx::query(
            "INSERT INTO checkpoints (id, run_id, span_id, sequence, kind, \
                 input_hash, input_payload_ref, output_payload_ref, created_at) \
             VALUES ('cp1', 'run_cp', 'span_cp', 0, 'model_step', \
                 'sha256:in', ?1, ?2, '2026-05-17T16:00:02Z')",
        )
        .bind(REF_C)
        .bind(REF_A)
        .execute(&pool)
        .await
        .unwrap();

        let got = find_blob_owner(&pool, "run_cp", REF_C).await.unwrap();
        assert_eq!(got.as_deref(), Some("full_debug"));
        let got = find_blob_owner(&pool, "run_cp", REF_A).await.unwrap();
        assert_eq!(got.as_deref(), Some("full_debug"));
    }

    #[tokio::test]
    async fn cross_run_isolation_holds() {
        // Two runs each own one ref; a query against run_a for run_b's
        // ref returns None, and vice versa.
        let pool = migrated_pool().await;
        seed_run(&pool, "run_a", "full_debug").await;
        seed_run(&pool, "run_b", "redacted").await;
        seed_span(&pool, "span_a", "run_a").await;
        seed_span(&pool, "span_b", "run_b").await;
        seed_model_call(&pool, "span_a", Some(REF_A), None).await;
        seed_model_call(&pool, "span_b", Some(REF_B), None).await;

        assert_eq!(
            find_blob_owner(&pool, "run_a", REF_A).await.unwrap().as_deref(),
            Some("full_debug"),
        );
        assert_eq!(
            find_blob_owner(&pool, "run_b", REF_B).await.unwrap().as_deref(),
            Some("redacted"),
        );
        assert!(find_blob_owner(&pool, "run_a", REF_B).await.unwrap().is_none());
        assert!(find_blob_owner(&pool, "run_b", REF_A).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn returns_hash_only_when_run_is_hash_only() {
        // The helper doesn't enforce policy — it returns the retention
        // mode as stored. The route is responsible for 403'ing on
        // hash_only. Test this explicitly so a future helper change
        // doesn't silently bypass that route-level check.
        let pool = migrated_pool().await;
        seed_run(&pool, "run_hash", "hash_only").await;
        seed_span(&pool, "span_h", "run_hash").await;
        // A blob ref would not normally exist under hash_only, but a
        // misconfigured producer or pre-migration row could leave one
        // dangling. The helper still reports the row's mode so the
        // route can refuse.
        seed_model_call(&pool, "span_h", Some(REF_A), None).await;

        let got = find_blob_owner(&pool, "run_hash", REF_A).await.unwrap();
        assert_eq!(got.as_deref(), Some("hash_only"));
    }
}
