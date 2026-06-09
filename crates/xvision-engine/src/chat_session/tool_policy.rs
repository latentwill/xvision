//! Three-state tool policy + Research/Act mode decision logic (Phase 2.3
//! SAFETY CORE).
//!
//! Two orthogonal safety dimensions gate every chat authoring tool before it
//! executes:
//!
//! 1. **Mode** (`research` | `act`) — a per-session flag persisted on
//!    `chat_sessions.mode`. Research is read-only: no WRITE tool may run. Act
//!    unlocks normal WRITE tools. Server-side enforcement reads the column from
//!    the DB; the client-sent value is never trusted.
//! 2. **Tool policy** (`enabled` + `auto_approve`) — a persisted per-tool,
//!    per-scope record in `tool_policies`. A disabled tool is hidden from the
//!    model and denied if requested anyway. Enabled WRITE tools auto-run by
//!    default in Act mode; operators can still disable or de-auto-approve
//!    individual tools.
//!
//! The classifier ([`classify`]) assigns each authoring verb a [`ToolClass`]
//! (Read / Write / Dangerous). The classification lives here — one place, not
//! scattered across the loop — so the policy surface and the enforcement hook
//! agree on what counts as a write.
//!
//! [`decide`] is a pure function over `(mode, class, policy)` returning a
//! [`ToolPolicyOutcome`]; it is exhaustively unit-tested. The CRUD methods
//! ([`ToolPolicyStore`]) persist operator overrides; rows exist only for tools
//! whose default has been changed.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

pub use xvision_observability::ToolPolicyOutcome;

/// The scope sentinel for a workspace-wide (non-per-user) policy row.
pub const GLOBAL_SCOPE: &str = "global";

/// Side-effect class of a chat authoring tool. Drives the default policy and
/// the Research/Act gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolClass {
    /// Inspection / list / get verbs. No persistent mutation. Always allowed,
    /// in any mode, auto-approved.
    Read,
    /// Authoring verbs that mutate a strategy/scenario/agent or launch work.
    /// Allowed only in Act mode; auto-approved by default.
    Write,
    /// Reserved for irreversible / high-blast-radius verbs. Disabled by
    /// default; an operator must explicitly enable + (optionally) auto-approve.
    Dangerous,
}

/// Persisted three-state policy for one tool. `None` row ⇒ use the
/// class default ([`ToolPolicy::default_for`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolPolicy {
    /// `false` ⇒ the tool is hidden from the model and denied if requested.
    pub enabled: bool,
    /// `true` ⇒ a Write tool runs without an approval round-trip.
    pub auto_approve: bool,
}

impl ToolPolicy {
    /// Default policy for a class:
    /// - Read → enabled + auto_approve
    /// - Write → enabled + auto_approve
    /// - Dangerous → disabled
    pub fn default_for(class: ToolClass) -> Self {
        match class {
            ToolClass::Read => ToolPolicy {
                enabled: true,
                auto_approve: true,
            },
            ToolClass::Write => ToolPolicy {
                enabled: true,
                auto_approve: true,
            },
            ToolClass::Dangerous => ToolPolicy {
                enabled: false,
                auto_approve: false,
            },
        }
    }
}

/// Classify a chat authoring tool by name. WRITE = the authoring/mutation
/// verbs (create_*, update_*, set_*, attach_*, clear_*, validate_draft,
/// run_eval, fetch_bars). READ = inspection/list/get/resolve verbs. Unknown
/// tool names default to WRITE — fail safe, since an unrecognised verb that
/// slips through should be gated by Act mode rather than silently allowed.
pub fn classify(tool_name: &str) -> ToolClass {
    match tool_name {
        // ── Read: inspection, listing, resolution. No mutation. ──────────
        "get_strategy"
        | "get_scenario"
        | "get_eval_run"
        | "get_eval_review"
        | "get_cli_job"
        | "get_cli_job_output"
        | "list_strategies"
        | "list_scenarios"
        | "list_eval_runs"
        | "list_eval_reviews"
        | "list_strategies_folder"
        | "read_strategies_file"
        | "list_strategy_ideas"
        | "resolve_strategy" => ToolClass::Read,

        // ── Write: authoring mutations + work launchers. ─────────────────
        "create_strategy"
        | "create_scenario"
        | "create_strategy_agent"
        | "update_slot"
        | "update_manifest"
        | "set_mechanical_param"
        | "set_risk_config"
        | "set_filter"
        | "clear_filter"
        | "attach_agent"
        | "validate_draft"
        | "run_eval"
        | "fetch_bars" => ToolClass::Write,

        // Unknown → fail safe as Write so it can't bypass the Act gate.
        _ => ToolClass::Write,
    }
}

/// Pure decision: given the session mode, the tool's class, and its effective
/// policy, what is the policy outcome?
///
/// Rules (in order):
/// 1. Disabled policy → `Denied` (regardless of class or mode).
/// 2. Write tool in `research` mode → `Denied` (read-only mode).
/// 3. Write tool in `act` mode → `AutoApproved` if `auto_approve`, else
///    `NeedsApproval` for explicit operator overrides.
/// 4. Read tool → `AutoApproved` (any mode).
/// 5. Dangerous behaves like Write here once enabled; its restriction is the
///    disabled-by-default policy handled by rule 1.
pub fn decide(mode: &str, class: ToolClass, policy: ToolPolicy) -> ToolPolicyOutcome {
    if !policy.enabled {
        return ToolPolicyOutcome::Denied;
    }
    match class {
        ToolClass::Read => ToolPolicyOutcome::AutoApproved,
        ToolClass::Write | ToolClass::Dangerous => {
            if mode != "act" {
                // research (or any non-act mode) is read-only.
                ToolPolicyOutcome::Denied
            } else if policy.auto_approve {
                ToolPolicyOutcome::AutoApproved
            } else {
                ToolPolicyOutcome::NeedsApproval
            }
        }
    }
}

/// Convenience: resolve the effective policy for `tool_name` — the persisted
/// override if one exists in `overrides`, else the class default.
pub fn effective_policy(tool_name: &str, overrides: &[(String, ToolPolicy)]) -> ToolPolicy {
    if let Some((_, p)) = overrides.iter().find(|(n, _)| n == tool_name) {
        *p
    } else {
        ToolPolicy::default_for(classify(tool_name))
    }
}

/// One persisted policy row joined with its tool name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolPolicyRow {
    pub tool_name: String,
    pub enabled: bool,
    pub auto_approve: bool,
}

/// Stateless CRUD over `tool_policies` (migration 043). Methods take
/// `&SqlitePool` so the same store is shared across handlers via AppState.
pub struct ToolPolicyStore;

impl ToolPolicyStore {
    /// Read every persisted policy override for a scope, newest tool name
    /// order undefined (callers map into a lookup). An empty result means
    /// every tool uses its class default.
    pub async fn get_policies(pool: &SqlitePool, user_scope: &str) -> Result<Vec<ToolPolicyRow>> {
        let rows: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT tool_name, enabled, auto_approve FROM tool_policies \
             WHERE user_scope = ?1 ORDER BY tool_name ASC",
        )
        .bind(user_scope)
        .fetch_all(pool)
        .await
        .context("load tool_policies for scope")?;
        Ok(rows
            .into_iter()
            .map(|(tool_name, enabled, auto_approve)| ToolPolicyRow {
                tool_name,
                enabled: enabled != 0,
                auto_approve: auto_approve != 0,
            })
            .collect())
    }

    /// Read one tool's persisted override, if any.
    pub async fn get_policy(
        pool: &SqlitePool,
        user_scope: &str,
        tool_name: &str,
    ) -> Result<Option<ToolPolicy>> {
        let row: Option<(i64, i64)> = sqlx::query_as(
            "SELECT enabled, auto_approve FROM tool_policies \
             WHERE user_scope = ?1 AND tool_name = ?2",
        )
        .bind(user_scope)
        .bind(tool_name)
        .fetch_optional(pool)
        .await
        .context("load tool_policy row")?;
        Ok(row.map(|(enabled, auto_approve)| ToolPolicy {
            enabled: enabled != 0,
            auto_approve: auto_approve != 0,
        }))
    }

    /// The effective policy for one tool: the persisted override if present,
    /// else the class default. The single resolution point enforcement reads.
    pub async fn effective(pool: &SqlitePool, user_scope: &str, tool_name: &str) -> Result<ToolPolicy> {
        Ok(match Self::get_policy(pool, user_scope, tool_name).await? {
            Some(p) => p,
            None => ToolPolicy::default_for(classify(tool_name)),
        })
    }

    /// Upsert a tool policy for a scope (insert or replace on the PK).
    pub async fn upsert_policy(
        pool: &SqlitePool,
        user_scope: &str,
        tool_name: &str,
        policy: ToolPolicy,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO tool_policies (user_scope, tool_name, enabled, auto_approve) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(user_scope, tool_name) DO UPDATE SET \
                enabled = excluded.enabled, auto_approve = excluded.auto_approve",
        )
        .bind(user_scope)
        .bind(tool_name)
        .bind(policy.enabled as i64)
        .bind(policy.auto_approve as i64)
        .execute(pool)
        .await
        .context("upsert tool_policies row")?;
        Ok(())
    }

    /// Remove a persisted override, reverting the tool to its class default.
    /// No-op if no override exists for the given scope + tool_name.
    pub async fn delete_policy(pool: &SqlitePool, user_scope: &str, tool_name: &str) -> Result<()> {
        sqlx::query("DELETE FROM tool_policies WHERE user_scope = ?1 AND tool_name = ?2")
            .bind(user_scope)
            .bind(tool_name)
            .execute(pool)
            .await
            .context("delete tool_policies row")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_default() -> ToolPolicy {
        ToolPolicy::default_for(ToolClass::Read)
    }
    fn write_default() -> ToolPolicy {
        ToolPolicy::default_for(ToolClass::Write)
    }

    #[test]
    fn classifier_marks_authoring_verbs_write() {
        for t in [
            "create_strategy",
            "create_scenario",
            "create_strategy_agent",
            "update_slot",
            "update_manifest",
            "set_mechanical_param",
            "set_risk_config",
            "set_filter",
            "clear_filter",
            "attach_agent",
            "validate_draft",
            "run_eval",
            "fetch_bars",
        ] {
            assert_eq!(classify(t), ToolClass::Write, "{t} should be Write");
        }
    }

    #[test]
    fn classifier_marks_inspection_verbs_read() {
        for t in [
            "get_strategy",
            "get_scenario",
            "get_eval_run",
            "get_eval_review",
            "get_cli_job",
            "get_cli_job_output",
            "list_strategies",
            "list_scenarios",
            "list_eval_runs",
            "list_eval_reviews",
            "list_strategies_folder",
            "read_strategies_file",
            "list_strategy_ideas",
            "resolve_strategy",
        ] {
            assert_eq!(classify(t), ToolClass::Read, "{t} should be Read");
        }
    }

    #[test]
    fn classifier_unknown_tool_fails_safe_to_write() {
        assert_eq!(classify("totally_made_up_verb"), ToolClass::Write);
    }

    #[test]
    fn defaults_match_spec() {
        assert_eq!(
            ToolPolicy::default_for(ToolClass::Read),
            ToolPolicy {
                enabled: true,
                auto_approve: true
            }
        );
        assert_eq!(
            ToolPolicy::default_for(ToolClass::Write),
            ToolPolicy {
                enabled: true,
                auto_approve: true
            }
        );
        assert_eq!(
            ToolPolicy::default_for(ToolClass::Dangerous),
            ToolPolicy {
                enabled: false,
                auto_approve: false
            }
        );
    }

    // ── decide() — every branch ──────────────────────────────────────────

    #[test]
    fn decide_disabled_is_denied_in_any_mode_or_class() {
        let disabled = ToolPolicy {
            enabled: false,
            auto_approve: true,
        };
        for mode in ["research", "act"] {
            for class in [ToolClass::Read, ToolClass::Write, ToolClass::Dangerous] {
                assert_eq!(
                    decide(mode, class, disabled),
                    ToolPolicyOutcome::Denied,
                    "disabled must deny ({mode}, {class:?})"
                );
            }
        }
    }

    #[test]
    fn decide_read_is_auto_approved_in_research_and_act() {
        assert_eq!(
            decide("research", ToolClass::Read, read_default()),
            ToolPolicyOutcome::AutoApproved
        );
        assert_eq!(
            decide("act", ToolClass::Read, read_default()),
            ToolPolicyOutcome::AutoApproved
        );
    }

    #[test]
    fn decide_write_in_research_is_denied() {
        assert_eq!(
            decide("research", ToolClass::Write, write_default()),
            ToolPolicyOutcome::Denied
        );
        // Even an auto_approve write is denied in research mode.
        let auto = ToolPolicy {
            enabled: true,
            auto_approve: true,
        };
        assert_eq!(
            decide("research", ToolClass::Write, auto),
            ToolPolicyOutcome::Denied
        );
    }

    #[test]
    fn decide_write_in_act_auto_approves_by_default() {
        assert_eq!(
            decide("act", ToolClass::Write, write_default()),
            ToolPolicyOutcome::AutoApproved
        );
    }

    #[test]
    fn decide_write_in_act_auto_approve_runs() {
        let auto = ToolPolicy {
            enabled: true,
            auto_approve: true,
        };
        assert_eq!(
            decide("act", ToolClass::Write, auto),
            ToolPolicyOutcome::AutoApproved
        );
    }

    #[test]
    fn decide_dangerous_enabled_behaves_like_write() {
        // Dangerous is disabled by default (rule 1), but once enabled it
        // follows the write rules.
        let enabled = ToolPolicy {
            enabled: true,
            auto_approve: false,
        };
        assert_eq!(
            decide("research", ToolClass::Dangerous, enabled),
            ToolPolicyOutcome::Denied
        );
        assert_eq!(
            decide("act", ToolClass::Dangerous, enabled),
            ToolPolicyOutcome::NeedsApproval
        );
        let auto = ToolPolicy {
            enabled: true,
            auto_approve: true,
        };
        assert_eq!(
            decide("act", ToolClass::Dangerous, auto),
            ToolPolicyOutcome::AutoApproved
        );
    }

    #[test]
    fn decide_unknown_mode_string_is_treated_as_read_only() {
        // Any mode that isn't exactly "act" must not unlock writes.
        assert_eq!(
            decide("", ToolClass::Write, write_default()),
            ToolPolicyOutcome::Denied
        );
        assert_eq!(
            decide("ACT", ToolClass::Write, write_default()),
            ToolPolicyOutcome::Denied,
            "mode comparison is case-sensitive; only lowercase 'act' unlocks"
        );
    }

    #[test]
    fn effective_policy_prefers_override_else_default() {
        let overrides = vec![(
            "create_strategy".to_string(),
            ToolPolicy {
                enabled: false,
                auto_approve: false,
            },
        )];
        // Override wins.
        assert_eq!(
            effective_policy("create_strategy", &overrides),
            ToolPolicy {
                enabled: false,
                auto_approve: false
            }
        );
        // No override → class default (Write).
        assert_eq!(effective_policy("update_slot", &overrides), write_default());
        // Read tool with no override → read default.
        assert_eq!(effective_policy("list_strategies", &overrides), read_default());
    }

    // ── CRUD round-trip ───────────────────────────────────────────────────

    async fn fresh_pool() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::query(include_str!("../../migrations/043_tool_policies.sql"))
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn crud_upsert_and_get_round_trip() {
        let pool = fresh_pool().await;
        // No override yet → effective is the class default.
        assert_eq!(
            ToolPolicyStore::effective(&pool, GLOBAL_SCOPE, "create_strategy")
                .await
                .unwrap(),
            write_default()
        );

        // Disable create_strategy globally.
        ToolPolicyStore::upsert_policy(
            &pool,
            GLOBAL_SCOPE,
            "create_strategy",
            ToolPolicy {
                enabled: false,
                auto_approve: false,
            },
        )
        .await
        .unwrap();
        let p = ToolPolicyStore::get_policy(&pool, GLOBAL_SCOPE, "create_strategy")
            .await
            .unwrap()
            .unwrap();
        assert!(!p.enabled);

        // Re-upsert: flip to enabled + auto_approve (replace on PK, not a dup row).
        ToolPolicyStore::upsert_policy(
            &pool,
            GLOBAL_SCOPE,
            "create_strategy",
            ToolPolicy {
                enabled: true,
                auto_approve: true,
            },
        )
        .await
        .unwrap();
        let p = ToolPolicyStore::effective(&pool, GLOBAL_SCOPE, "create_strategy")
            .await
            .unwrap();
        assert_eq!(
            p,
            ToolPolicy {
                enabled: true,
                auto_approve: true
            }
        );

        let all = ToolPolicyStore::get_policies(&pool, GLOBAL_SCOPE).await.unwrap();
        assert_eq!(all.len(), 1, "PK upsert must not create a duplicate row");
        assert_eq!(all[0].tool_name, "create_strategy");
    }

    #[tokio::test]
    async fn scopes_are_isolated() {
        let pool = fresh_pool().await;
        ToolPolicyStore::upsert_policy(
            &pool,
            "user_42",
            "run_eval",
            ToolPolicy {
                enabled: false,
                auto_approve: false,
            },
        )
        .await
        .unwrap();
        // Global scope unaffected.
        assert!(ToolPolicyStore::get_policy(&pool, GLOBAL_SCOPE, "run_eval")
            .await
            .unwrap()
            .is_none());
        assert!(ToolPolicyStore::get_policy(&pool, "user_42", "run_eval")
            .await
            .unwrap()
            .is_some());
    }
}
