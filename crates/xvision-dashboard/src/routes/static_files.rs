//! SPA fallback handler — serves packaged assets, falling back to `index.html`
//! so React Router can take over deep-link routes.

use axum::{
    body::Body,
    extract::Path,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::embed::Assets;

pub async fn serve_index() -> Response {
    serve_path("index.html").await
}

pub async fn serve_static(Path(path): Path<String>) -> Response {
    // `rust-embed` stores files relative to the configured `#[folder = "static/"]`,
    // so the SPA's `static/assets/index-*.js` ends up under the key
    // `assets/index-*.js`. Axum's `Path<String>` extractor strips the `/assets/`
    // route prefix, so reattach it before looking up the embedded asset.
    serve_path(&format!("assets/{path}")).await
}

pub async fn fallback(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if uri.path().starts_with("/api/") {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "code": "not_found", "message": format!("no route for {}", uri.path()) })),
        )
            .into_response();
    }
    if path.is_empty() {
        return serve_path("index.html").await;
    }
    if let Some(resp) = try_serve_path(path).await {
        return resp;
    }
    serve_path("index.html").await
}

async fn serve_path(path: &str) -> Response {
    match try_serve_path(path).await {
        Some(resp) => resp,
        None => (StatusCode::NOT_FOUND, "asset not found").into_response(),
    }
}

async fn try_serve_path(path: &str) -> Option<Response> {
    let asset = Assets::get(path)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Some(
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime.as_ref())
            .body(Body::from(asset.data.into_owned()))
            .expect("response builder"),
    )
}
