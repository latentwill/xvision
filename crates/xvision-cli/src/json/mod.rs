//! Shared output-format primitives for `xvn <object> get` commands.
//!
//! Anchors the per-object JSON shape contract documented in q15 §6 /
//! `team/contracts/q15-object-json-output.md`. Each `xvn agent | scenario
//! | strategy get <id>` reuses [`ObjectFormat`] and [`emit_object`] so
//! the format flag stays consistent across the three verbs.
//!
//! Why a shared module: a future track may need to add an alternate
//! format (TOML, YAML) or a wrapper envelope. Doing the rendering in
//! one place keeps the three CLI surfaces from drifting.

pub mod object_shapes;

pub use object_shapes::{emit_object, ObjectFormat};
