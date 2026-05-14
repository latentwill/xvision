//! SPA asset loader.
//!
//! `frontend/web` builds with Vite into `crates/xvision-dashboard/static/`; that
//! directory is gitignored and populated by `pnpm build` (or by `build.rs` in a
//! later phase). `rust-embed` baked the assets into the binary at compile time.
//!
//! The directory must exist even for compile-only CI checks. Docker deploy builds
//! the real SPA first and copies it here before compiling the dashboard binary.

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "static/"]
pub struct Assets;
