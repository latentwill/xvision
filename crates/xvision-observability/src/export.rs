//! Read-side export surface for the canonical agent-run ledger.
//!
//! This module loads a single `agent_runs` row plus its dependent
//! span/checkpoint/model_call/tool_call/approval/sandbox/note/artifact/event
//! rows from an open SQLite pool, and shapes them into two stable
//! deliverables:
//!
//! - [`AgentRunExport`] — serializes to the `xvn.agent_run.v1` JSON
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

use crate::rows::{
    AgentRunRow, ApprovalRow, ModelCallRow, SandboxResultRow, SpanRow,
    SupervisorNoteRow, ToolCallRow,
};

/// Schema-version tag stamped onto every export. **Do not** mutate v1
/// in place — future shape changes get a new tag (`xvn.agent_run.v2`)
/// and a new struct.
pub const SCHEMA_VERSION: &str = "xvn.agent_run.v1";

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

/// The `xvn.agent_run.v1` payload. Top-level field order follows the
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
    pub spans: Vec<SpanNode>,
    pub model_calls: Vec<ModelCallRow>,
    pub tool_calls: Vec<ToolCallRow>,
    pub approvals: Vec<ApprovalRow>,
    pub sandbox_results: Vec<SandboxResultRow>,
    pub supervisor_notes: Vec<SupervisorNoteRow>,
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
    let run = load_agent_run(pool, run_id).await?;
    let span_rows = load_spans(pool, run_id).await?;
    let model_calls = load_model_calls(pool, run_id).await?;
    let tool_calls = load_tool_calls(pool, run_id).await?;
    let approvals = load_approvals(pool, run_id).await?;
    let sandbox_results = load_sandbox_results(pool, run_id).await?;
    let supervisor_notes = load_supervisor_notes(pool, run_id).await?;
    let final_artifact = if let Some(ref aid) = run.final_artifact_id {
        load_artifact(pool, aid).await?
    } else {
        None
    };

    let totals = compute_totals(&model_calls, &tool_calls, &approvals);
    let spans = into_tree(span_rows);

    let mcp_servers = parse_optional_json(run.mcp_servers_json.as_deref(), "mcp_servers_json")?;
    let skills = parse_optional_json(run.skills_json.as_deref(), "skills_json")?;

    Ok(AgentRunExport {
        schema_version: SCHEMA_VERSION,
        run_id: run.id,
        objective: run.objective,
        strategy_id: run.strategy_id,
        eval_run_id: run.eval_run_id,
        status: run.status,
        retention_mode: run.retention_mode,
        started_at: run.started_at,
        finished_at: run.finished_at,
        otel_trace_id: run.otel_trace_id,
        totals,
        spans,
        model_calls,
        tool_calls,
        approvals,
        sandbox_results,
        supervisor_notes,
        final_artifact,
        sidecar_version: run.sidecar_version,
        cline_sdk_version: run.cline_sdk_version,
        protocol_version: run.protocol_version,
        mcp_servers,
        skills,
    })
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
    if let Some(ref sid) = export.strategy_id {
        let _ = writeln!(out, "- Strategy: {sid}");
    }
    if let Some(ref eid) = export.eval_run_id {
        let _ = writeln!(out, "- Eval run: {eid}");
    }
    let _ = writeln!(
        out,
        "- Started at: {}",
        export.started_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
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

    // Tool calls.
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

async fn load_model_calls(
    pool: &SqlitePool,
    run_id: &str,
) -> Result<Vec<ModelCallRow>, ExportError> {
    let rows: Vec<SqliteRow> = sqlx::query(
        "SELECT mc.span_id, mc.provider, mc.model, mc.input_token_count, \
                mc.output_token_count, mc.cost_usd, mc.prompt_hash, \
                mc.response_hash, mc.prompt_payload_ref, mc.response_payload_ref, \
                mc.tool_calls_requested, mc.capability_path \
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
            Ok(ModelCallRow {
                span_id: r.try_get("span_id")?,
                provider: r.try_get("provider")?,
                model: r.try_get("model")?,
                input_token_count: r.try_get("input_token_count")?,
                output_token_count: r.try_get("output_token_count")?,
                cost_usd: r.try_get("cost_usd")?,
                prompt_hash: r.try_get("prompt_hash")?,
                response_hash: r.try_get("response_hash")?,
                prompt_payload_ref: r.try_get("prompt_payload_ref")?,
                response_payload_ref: r.try_get("response_payload_ref")?,
                tool_calls_requested: r.try_get("tool_calls_requested")?,
                capability_path: r.try_get("capability_path")?,
            })
        })
        .collect()
}

async fn load_tool_calls(
    pool: &SqlitePool,
    run_id: &str,
) -> Result<Vec<ToolCallRow>, ExportError> {
    let rows: Vec<SqliteRow> = sqlx::query(
        "SELECT tc.span_id, tc.tool_name, tc.origin, tc.tool_version, tc.tool_hash, \
                tc.input_hash, tc.output_hash, tc.input_payload_ref, tc.output_payload_ref, \
                tc.side_effect_level, tc.risk_level, tc.requires_approval, tc.approval_id, \
                tc.exit_code, tc.is_run_terminator \
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
            Ok(ToolCallRow {
                span_id: r.try_get("span_id")?,
                tool_name: r.try_get("tool_name")?,
                origin: r.try_get("origin")?,
                tool_version: r.try_get("tool_version")?,
                tool_hash: r.try_get("tool_hash")?,
                input_hash: r.try_get("input_hash")?,
                output_hash: r.try_get("output_hash")?,
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

async fn load_sandbox_results(
    pool: &SqlitePool,
    run_id: &str,
) -> Result<Vec<SandboxResultRow>, ExportError> {
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

async fn load_artifact(
    pool: &SqlitePool,
    artifact_id: &str,
) -> Result<Option<FinalArtifact>, ExportError> {
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
        .map(Some)
        .map_err(|source| ExportError::InvalidJson { column, source })
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
    leaves_first.sort_by(|a, b| {
        b.started_at
            .cmp(&a.started_at)
            .then_with(|| b.id.cmp(&a.id))
    });

    for row in leaves_first {
        let node = SpanNode {
            children: children_of.remove(&row.id).unwrap_or_default(),
            row,
        };
        match &node.row.parent_span_id {
            Some(parent_id) if id_set.contains(parent_id) => {
                children_of
                    .entry(parent_id.clone())
                    .or_default()
                    .push(node);
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
