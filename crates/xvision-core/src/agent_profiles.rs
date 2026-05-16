//! Canonical review-agent persona identifiers and persona-kind enum.
//!
//! The `agent_profiles` table is owned by `xvision-engine` (because it is
//! tied to engine-side migrations and SQLx access). The persona *names*,
//! however, are part of the public contract between the engine, the
//! API/CLI surface, and any future UI selector — code that needs to
//! match on persona kind without depending on the engine crate lives
//! here.
//!
//! Spec: `docs/superpowers/specs/2026-05-15-eval-review-agent.md` — the
//! four canonical personas are `fast-trader-agent`, `reasoning-agent`,
//! `risk-agent`, `research-agent`. Migration 016 seeds them.

use serde::{Deserialize, Serialize};

/// Canonical row id for the Fast Trader persona seeded by migration 016.
pub const FAST_TRADER_AGENT_ID: &str = "fast-trader-agent";
/// Canonical row id for the Reasoning persona seeded by migration 016.
pub const REASONING_AGENT_ID: &str = "reasoning-agent";
/// Canonical row id for the Risk persona seeded by migration 016.
pub const RISK_AGENT_ID: &str = "risk-agent";
/// Canonical row id for the Research persona seeded by migration 016.
pub const RESEARCH_AGENT_ID: &str = "research-agent";

/// All canonical persona ids, in the order the UI selector should
/// present them (matches the spec's "Agent Profiles" section ordering).
pub const CANONICAL_AGENT_PROFILE_IDS: &[&str] = &[
    FAST_TRADER_AGENT_ID,
    REASONING_AGENT_ID,
    RISK_AGENT_ID,
    RESEARCH_AGENT_ID,
];

/// The persona "type" column on the `agent_profiles` row. Open enum: a
/// `Custom(String)` variant carries any operator-defined persona without
/// requiring a migration. Wire format mirrors the spec's bare strings
/// (`fast-trader`, `reasoning`, `risk`, `research`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case", untagged)]
pub enum PersonaKind {
    Known(KnownPersona),
    Custom(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum KnownPersona {
    FastTrader,
    Reasoning,
    Risk,
    Research,
}

impl KnownPersona {
    pub fn as_str(self) -> &'static str {
        match self {
            KnownPersona::FastTrader => "fast-trader",
            KnownPersona::Reasoning => "reasoning",
            KnownPersona::Risk => "risk",
            KnownPersona::Research => "research",
        }
    }

    /// Canonical row id for this persona (matches migration 016 seeds).
    pub fn canonical_id(self) -> &'static str {
        match self {
            KnownPersona::FastTrader => FAST_TRADER_AGENT_ID,
            KnownPersona::Reasoning => REASONING_AGENT_ID,
            KnownPersona::Risk => RISK_AGENT_ID,
            KnownPersona::Research => RESEARCH_AGENT_ID,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "fast-trader" => Some(KnownPersona::FastTrader),
            "reasoning" => Some(KnownPersona::Reasoning),
            "risk" => Some(KnownPersona::Risk),
            "research" => Some(KnownPersona::Research),
            _ => None,
        }
    }
}

impl PersonaKind {
    /// Parse a `type` column value. Falls back to `Custom` for any
    /// operator-defined persona that hasn't been promoted to a canonical
    /// variant.
    pub fn parse(s: &str) -> Self {
        match KnownPersona::parse(s) {
            Some(known) => PersonaKind::Known(known),
            None => PersonaKind::Custom(s.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            PersonaKind::Known(k) => k.as_str(),
            PersonaKind::Custom(s) => s.as_str(),
        }
    }

    /// Returns `Some(known)` when this persona is one of the four
    /// canonical kinds, `None` for operator-defined customs.
    pub fn known(&self) -> Option<KnownPersona> {
        match self {
            PersonaKind::Known(k) => Some(*k),
            PersonaKind::Custom(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_ids_match_migration_016_seeds() {
        // These string literals are the contract migration 016 binds to;
        // if anyone renames a seed, this test breaks the build.
        assert_eq!(FAST_TRADER_AGENT_ID, "fast-trader-agent");
        assert_eq!(REASONING_AGENT_ID, "reasoning-agent");
        assert_eq!(RISK_AGENT_ID, "risk-agent");
        assert_eq!(RESEARCH_AGENT_ID, "research-agent");
        assert_eq!(CANONICAL_AGENT_PROFILE_IDS.len(), 4);
    }

    #[test]
    fn known_persona_round_trips_str() {
        for kind in [
            KnownPersona::FastTrader,
            KnownPersona::Reasoning,
            KnownPersona::Risk,
            KnownPersona::Research,
        ] {
            assert_eq!(KnownPersona::parse(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn known_persona_canonical_id_aligns_with_constants() {
        assert_eq!(KnownPersona::FastTrader.canonical_id(), FAST_TRADER_AGENT_ID);
        assert_eq!(KnownPersona::Reasoning.canonical_id(), REASONING_AGENT_ID);
        assert_eq!(KnownPersona::Risk.canonical_id(), RISK_AGENT_ID);
        assert_eq!(KnownPersona::Research.canonical_id(), RESEARCH_AGENT_ID);
    }

    #[test]
    fn persona_kind_parses_known_and_custom() {
        let known = PersonaKind::parse("reasoning");
        assert_eq!(known.known(), Some(KnownPersona::Reasoning));

        let custom = PersonaKind::parse("operator-defined");
        assert_eq!(custom.known(), None);
        assert_eq!(custom.as_str(), "operator-defined");
    }

    #[test]
    fn persona_kind_unknown_str_falls_back_to_custom() {
        let kind = PersonaKind::parse("amazing");
        assert!(matches!(kind, PersonaKind::Custom(_)));
        assert_eq!(kind.as_str(), "amazing");
    }
}
