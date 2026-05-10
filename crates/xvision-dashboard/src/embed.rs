//! SPA asset loader.
//!
//! `frontend/web` builds with Vite into `crates/xvision-dashboard/static/`; that
//! directory is gitignored and populated by `pnpm build` (or by `build.rs` in a
//! later phase). `rust-embed` baked the assets into the binary at compile time.
//!
//! For Phase A scaffolding the directory may be empty — `rust-embed` tolerates
//! a missing folder by failing the include only when an asset is requested at
//! runtime, so the binary still compiles before the frontend is built.

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "static/"]
pub struct Assets;
