//! `POST /api/strategies-folder/import` — accept `multipart/form-data`
//! uploads from the `/strategies-folder` dashboard route. Re-uses the
//! engine-side `strategies_folder::import_bytes` so the CLI and HTTP
//! surfaces share identical type-allowlist, size-cap, and summary-sidecar
//! semantics.
//!
//! Form fields:
//! - `file` (required, exactly one — first file part is taken; extras
//!   are ignored). The multipart `filename` is what gets sanitised and
//!   placed under the destination subfolder.
//! - `to` (optional) — destination subfolder override. Must be in the
//!   allowlist; missing → resolved by extension.
//! - `no_clobber` (optional, "true" / "false") — defaults to false
//!   (overwrite).
//!
//! Response shape: `{ entry, summary?, findings[] }` — same struct as
//! `strategies_folder::ImportOutcome`.
//!
//! Path safety + type allowlist + size cap are all enforced inside
//! `strategies_folder::import_bytes` — the route only deals with HTTP
//! framing.

use axum::extract::{Multipart, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use xvision_engine::strategies_folder::{self, FolderEntry, ImportFinding, ImportOptions, MAX_IMPORT_BYTES};

use crate::error::DashboardError;
use crate::state::AppState;

/// JSON response. Mirrors `ImportOutcome` but is owned here so the
/// dashboard can evolve its surface without bumping the engine type.
#[derive(Debug, Serialize, Deserialize)]
pub struct ImportResponse {
    pub entry: FolderEntry,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<FolderEntry>,
    pub findings: Vec<ImportFinding>,
}

/// `GET /api/strategies-folder/list?subfolder=<name>` — enumerate the
/// strategies folder for the dashboard surface. Thin wrapper around
/// `strategies_folder::list`; the subfolder filter is optional.
#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    pub subfolder: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListResponse {
    pub items: Vec<FolderEntry>,
}

pub async fn get_list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<ListResponse>, DashboardError> {
    let items = strategies_folder::list(&state.api_context(), params.subfolder.as_deref()).await?;
    Ok(Json(ListResponse { items }))
}

pub async fn post_import(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<ImportResponse>, DashboardError> {
    let mut file_bytes: Option<(String, Vec<u8>)> = None;
    let mut to: Option<String> = None;
    let mut no_clobber = false;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| DashboardError::Validation {
            field: "multipart".into(),
            msg: format!("read field: {e}"),
        })?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                if file_bytes.is_some() {
                    // First file wins; ignore extras silently to match
                    // typical browser multipart behavior.
                    continue;
                }
                let filename = field.file_name().unwrap_or("").to_string();
                if filename.is_empty() {
                    return Err(DashboardError::Validation {
                        field: "file".into(),
                        msg: "file part missing filename".into(),
                    });
                }
                let bytes = field.bytes().await.map_err(|e| DashboardError::Validation {
                    field: "file".into(),
                    msg: format!("read bytes: {e}"),
                })?;
                if (bytes.len() as u64) > MAX_IMPORT_BYTES {
                    return Err(DashboardError::Validation {
                        field: "file".into(),
                        msg: format!(
                            "import_too_large: file is {} bytes; max is {} bytes",
                            bytes.len(),
                            MAX_IMPORT_BYTES
                        ),
                    });
                }
                file_bytes = Some((filename, bytes.to_vec()));
            }
            "to" => {
                let raw = field.text().await.map_err(|e| DashboardError::Validation {
                    field: "to".into(),
                    msg: format!("read text: {e}"),
                })?;
                let trimmed = raw.trim();
                if !trimmed.is_empty() {
                    to = Some(trimmed.to_string());
                }
            }
            "no_clobber" => {
                let raw = field.text().await.map_err(|e| DashboardError::Validation {
                    field: "no_clobber".into(),
                    msg: format!("read text: {e}"),
                })?;
                no_clobber = matches!(
                    raw.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                );
            }
            _ => {
                // Drain unknown fields so the connection stays in sync.
                let _ = field.bytes().await;
            }
        }
    }

    let (filename, bytes) = file_bytes.ok_or_else(|| DashboardError::Validation {
        field: "file".into(),
        msg: "missing `file` part".into(),
    })?;

    let outcome = strategies_folder::import_bytes(
        &state.api_context(),
        &filename,
        &bytes,
        ImportOptions {
            subfolder: to,
            clobber: !no_clobber,
        },
    )
    .await?;

    Ok(Json(ImportResponse {
        entry: outcome.entry,
        summary: outcome.summary,
        findings: outcome.findings,
    }))
}
