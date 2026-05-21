//! Regression guard: the three scaffolding artifacts that confused a QA
//! operator during the xvnej-app rerun must never re-enter the seed path.
//!
//! Background (team/intake/2026-05-21-eval-honesty-and-agent-graph.md):
//!
//! * Strategy `Test Strategy` (id `01KS204HPAWFXP72TXVXNCTYPX`)
//! * Agent   `Template-ish Agent` (id `01KS1VD3V3PNYN4E8B1BCJ4N11`)
//! * Agent   `Template Mean Reversion Agent` (id `01KS1VDHFKZGJJ59S7XQPCA45C`)
//!
//! These showed up in the eval catalog next to real strategies and were
//! mistaken for shipped templates. They are DB-only artifacts in existing
//! workspaces; this test locks the seed source so they can never be
//! re-introduced via `example_strategies()` or `builtin_templates()`.

use xvision_engine::agents::templates::builtin_templates;
use xvision_engine::strategies::templates::example_strategies;

/// Names that must not appear in any seeded strategy or agent template.
///
/// Matching is case-insensitive and checks both `display_name` (strategies)
/// and `name` (agent templates) fields.
const BANNED_SEED_NAMES: &[&str] = &[
    "Test Strategy",
    "Template-ish Agent",
    "Template Mean Reversion Agent",
];

#[test]
fn example_strategies_do_not_contain_scaffold_names() {
    let strategies = example_strategies();
    for strategy in &strategies {
        let display_name_lower = strategy.manifest.display_name.to_lowercase();
        for banned in BANNED_SEED_NAMES {
            assert!(
                !display_name_lower.contains(&banned.to_lowercase()),
                "example strategy '{}' (id={}) matches banned scaffold name '{}'.\n\
                 These artifacts must not be seeded into new operator workspaces.\n\
                 See team/intake/2026-05-21-eval-honesty-and-agent-graph.md.",
                strategy.manifest.display_name,
                strategy.manifest.id,
                banned,
            );
        }
    }
}

#[test]
fn agent_templates_do_not_contain_scaffold_names() {
    let templates = builtin_templates();
    for tpl in &templates {
        let name_lower = tpl.name.to_lowercase();
        for banned in BANNED_SEED_NAMES {
            assert!(
                !name_lower.contains(&banned.to_lowercase()),
                "agent template '{}' (id={}) matches banned scaffold name '{}'.\n\
                 These artifacts must not be seeded into new operator workspaces.\n\
                 See team/intake/2026-05-21-eval-honesty-and-agent-graph.md.",
                tpl.name,
                tpl.id,
                banned,
            );
        }
    }
}

#[test]
fn example_strategy_ids_do_not_contain_scaffold_ulids() {
    /// ULIDs of the three specific artifacts identified in the QA report.
    const BANNED_IDS: &[&str] = &[
        "01KS204HPAWFXP72TXVXNCTYPX",
        "01KS1VD3V3PNYN4E8B1BCJ4N11",
        "01KS1VDHFKZGJJ59S7XQPCA45C",
    ];

    let strategies = example_strategies();
    for strategy in &strategies {
        for banned_id in BANNED_IDS {
            assert_ne!(
                strategy.manifest.id.as_str(),
                *banned_id,
                "example strategy id '{}' matches banned scaffold ULID.\n\
                 See team/intake/2026-05-21-eval-honesty-and-agent-graph.md.",
                strategy.manifest.id,
            );
        }
    }
}
