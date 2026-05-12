//! Skills registry — workspace-level reusable modules referenced by
//! agent slots. v1 ships CRUD only; runtime application of skills (where
//! a skill actually does something during an agent's execution) lands
//! per-kind as specific behaviors are wired up.
//!
//! See `docs/superpowers/plans/2026-05-11-agents-page-v1.md` §Skills.

pub mod model;
pub mod store;

pub use model::{Skill, SkillKind};
pub use store::{NewSkill, SkillStore, UpdateSkill};
