//! `AgentStore` — sqlx-backed CRUD for agents + their slots.
//!
//! Mirrors the pattern from `eval::store::RunStore`: the store does not
//! manage the SQLite pool; callers construct an `ApiContext` (which owns
//! the pool + has migrations applied) and pass `ctx.db.clone()` here.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use ulid::Ulid;

use crate::agents::model::{Agent, AgentSlot, InputsPolicy};
use crate::agents::validate::validate_agent_for_save;
use crate::agents::validator::{validate_prompt_schema_slots, PromptSchemaDriftError};

#[derive(Debug, Clone)]
pub struct AgentStore {
    pool: SqlitePool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewAgent {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub slots: Vec<AgentSlot>,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateAgent {
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub slots: Option<Vec<AgentSlot>>,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct ListFilter {
    pub include_archived: bool,
    pub name_contains: Option<String>,
    pub limit: Option<i64>,
}

impl AgentStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, new: NewAgent) -> Result<String> {
        // Save-gate: run the content-quality checks before touching the DB.
        // Build a temporary Agent so validate_agent_for_save has the full
        // picture (name + all slot prompts).
        {
            let now = Utc::now();
            let probe = Agent {
                agent_id: String::new(),
                name: new.name.clone(),
                description: new.description.clone(),
                tags: new.tags.clone(),
                slots: new.slots.clone(),
                archived: false,
                created_at: now,
                updated_at: now,
            };
            validate_agent_for_save(&probe)
                .map_err(|msg| anyhow::anyhow!("save validation failed: {msg}"))?;
        }
        // F-5 pre-persist drift gate: refuse agents whose prompts
        // reference tools that aren't registered for the slot or
        // declare an `Allowed actions:` list that drifts from the
        // `trader_output` schema enum. See
        // `crates/xvision-engine/src/agents/validator.rs`.
        validate_prompt_schema_slots(&new.slots).map_err(PromptSchemaDriftError::into_anyhow)?;

        let id = Ulid::new().to_string();
        let now = Utc::now().to_rfc3339();
        let tags_json = serde_json::to_string(&new.tags).context("serialize tags")?;

        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "INSERT INTO agents (agent_id, name, description, tags_json, archived, created_at, updated_at) \
             VALUES (?, ?, ?, ?, 0, ?, ?)",
        )
        .bind(&id)
        .bind(&new.name)
        .bind(&new.description)
        .bind(&tags_json)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .with_context(|| format!("insert agent name={}", new.name))?;

        for (idx, slot) in new.slots.iter().enumerate() {
            insert_slot(&mut tx, &id, idx as i64, slot).await?;
        }

        tx.commit().await?;
        Ok(id)
    }

    pub async fn get(&self, agent_id: &str) -> Result<Option<Agent>> {
        let row = sqlx::query(
            "SELECT agent_id, name, description, tags_json, archived, created_at, updated_at \
             FROM agents WHERE agent_id = ?",
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else { return Ok(None) };

        let slots = self.load_slots(agent_id).await?;
        Ok(Some(row_to_agent(row, slots)?))
    }

    pub async fn list(&self, filter: ListFilter) -> Result<Vec<Agent>> {
        let mut sql = String::from(
            "SELECT agent_id, name, description, tags_json, archived, created_at, updated_at \
             FROM agents WHERE 1=1",
        );
        if !filter.include_archived {
            sql.push_str(" AND archived = 0");
        }
        if filter.name_contains.is_some() {
            sql.push_str(" AND name LIKE ?");
        }
        sql.push_str(" ORDER BY updated_at DESC");
        if filter.limit.is_some() {
            sql.push_str(" LIMIT ?");
        }

        let mut q = sqlx::query(&sql);
        if let Some(ref needle) = filter.name_contains {
            q = q.bind(format!("%{}%", needle));
        }
        if let Some(limit) = filter.limit {
            q = q.bind(limit);
        }

        let rows = q.fetch_all(&self.pool).await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let agent_id: String = row.try_get("agent_id")?;
            let slots = self.load_slots(&agent_id).await?;
            out.push(row_to_agent(row, slots)?);
        }
        Ok(out)
    }

    pub async fn update(&self, agent_id: &str, patch: UpdateAgent) -> Result<Option<Agent>> {
        // Verify it exists first; return None if not.
        let existing = self.get(agent_id).await?;
        let Some(ref existing_agent) = existing else {
            return Ok(None);
        };

        // Save-gate: build the post-patch view and run content-quality checks
        // before touching the DB. Only the fields being patched need merging.
        {
            let probe = Agent {
                agent_id: existing_agent.agent_id.clone(),
                name: patch.name.clone().unwrap_or_else(|| existing_agent.name.clone()),
                description: patch
                    .description
                    .clone()
                    .unwrap_or_else(|| existing_agent.description.clone()),
                tags: patch.tags.clone().unwrap_or_else(|| existing_agent.tags.clone()),
                slots: patch
                    .slots
                    .clone()
                    .unwrap_or_else(|| existing_agent.slots.clone()),
                archived: existing_agent.archived,
                created_at: existing_agent.created_at,
                updated_at: existing_agent.updated_at,
            };
            validate_agent_for_save(&probe)
                .map_err(|msg| anyhow::anyhow!("save validation failed: {msg}"))?;
        }

        let now = Utc::now().to_rfc3339();
        let mut tx = self.pool.begin().await?;

        if let Some(name) = patch.name {
            sqlx::query("UPDATE agents SET name = ?, updated_at = ? WHERE agent_id = ?")
                .bind(name)
                .bind(&now)
                .bind(agent_id)
                .execute(&mut *tx)
                .await?;
        }
        if let Some(description) = patch.description {
            sqlx::query("UPDATE agents SET description = ?, updated_at = ? WHERE agent_id = ?")
                .bind(description)
                .bind(&now)
                .bind(agent_id)
                .execute(&mut *tx)
                .await?;
        }
        if let Some(tags) = patch.tags {
            let json = serde_json::to_string(&tags).context("serialize tags")?;
            sqlx::query("UPDATE agents SET tags_json = ?, updated_at = ? WHERE agent_id = ?")
                .bind(json)
                .bind(&now)
                .bind(agent_id)
                .execute(&mut *tx)
                .await?;
        }
        if let Some(slots) = patch.slots {
            // F-5 pre-persist drift gate (same rules as `create`).
            // Validate before deleting the old slot rows so a rejected
            // update leaves the previous version intact.
            validate_prompt_schema_slots(&slots).map_err(PromptSchemaDriftError::into_anyhow)?;
            // Replace all slots — simpler than diffing in v1.
            sqlx::query("DELETE FROM agent_slots WHERE agent_id = ?")
                .bind(agent_id)
                .execute(&mut *tx)
                .await?;
            for (idx, slot) in slots.iter().enumerate() {
                insert_slot(&mut tx, agent_id, idx as i64, slot).await?;
            }
            sqlx::query("UPDATE agents SET updated_at = ? WHERE agent_id = ?")
                .bind(&now)
                .bind(agent_id)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        self.get(agent_id).await
    }

    pub async fn archive(&self, agent_id: &str) -> Result<bool> {
        let now = Utc::now().to_rfc3339();
        let result =
            sqlx::query("UPDATE agents SET archived = 1, updated_at = ? WHERE agent_id = ? AND archived = 0")
                .bind(&now)
                .bind(agent_id)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn name_exists(&self, name: &str, excluding_id: Option<&str>) -> Result<bool> {
        let row = match excluding_id {
            Some(id) => {
                sqlx::query("SELECT 1 FROM agents WHERE name = ? AND agent_id != ? LIMIT 1")
                    .bind(name)
                    .bind(id)
                    .fetch_optional(&self.pool)
                    .await?
            }
            None => {
                sqlx::query("SELECT 1 FROM agents WHERE name = ? LIMIT 1")
                    .bind(name)
                    .fetch_optional(&self.pool)
                    .await?
            }
        };
        Ok(row.is_some())
    }

    async fn load_slots(&self, agent_id: &str) -> Result<Vec<AgentSlot>> {
        let rows = sqlx::query(
            "SELECT name, provider, model, system_prompt, skill_ids_json, max_tokens, prompt_version, inputs_policy, bar_history_limit \
             FROM agent_slots WHERE agent_id = ? ORDER BY slot_index ASC",
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let skill_ids_json: String = row.try_get("skill_ids_json")?;
            let skill_ids: Vec<String> =
                serde_json::from_str(&skill_ids_json).context("parse skill_ids_json")?;
            // `max_tokens` is stored as a non-null integer; `0` is the
            // sentinel for "unset" so the resolver pulls from the model's
            // metadata at dispatch time (q15 §1).
            let stored: i64 = row.try_get("max_tokens")?;
            let max_tokens = if stored <= 0 { None } else { Some(stored as u32) };
            // `inputs_policy` was added in migration 020 with default
            // `'raw'`; unknown / unparseable values also fall back to
            // `Raw` via `parse_or_raw` so the read path never panics
            // on a future typo.
            let inputs_policy_s: String = row.try_get("inputs_policy").unwrap_or_default();
            let inputs_policy = InputsPolicy::parse_or_raw(&inputs_policy_s);
            // `bar_history_limit` was added in migration 022 as a
            // NULLable INTEGER. `None` (the default) preserves today's
            // behavior; non-positive ints are treated as `None` so a
            // stray `0` can't accidentally clear the trader's view.
            let stored_limit: Option<i64> = row.try_get("bar_history_limit").ok().flatten();
            let bar_history_limit = match stored_limit {
                Some(n) if n > 0 => Some(n as u32),
                _ => None,
            };
            out.push(AgentSlot {
                name: row.try_get("name")?,
                provider: row.try_get("provider")?,
                model: row.try_get("model")?,
                system_prompt: row.try_get("system_prompt")?,
                skill_ids,
                max_tokens,
                temperature: None,
                prompt_version: row.try_get("prompt_version").unwrap_or_default(),
                inputs_policy,
                bar_history_limit,
            });
        }
        Ok(out)
    }
}

async fn insert_slot(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    agent_id: &str,
    idx: i64,
    slot: &AgentSlot,
) -> Result<()> {
    let skill_ids_json = serde_json::to_string(&slot.skill_ids).context("serialize skill_ids")?;
    // Always recompute server-side from `system_prompt`; any value the
    // client sent on `slot.prompt_version` is silently overridden so the
    // column is a true content digest, not free-text metadata. See F-3.
    let prompt_version = AgentSlot::compute_prompt_version(&slot.system_prompt);
    // F-8: `bar_history_limit` persists as a NULLable INTEGER
    // (migration 022). `None` and non-positive ints both map to SQL
    // NULL so the read path's "Some(0) → None" normalisation has a
    // round-trippable wire form.
    let bar_history_limit_db: Option<i64> =
        slot.bar_history_limit
            .and_then(|n| if n == 0 { None } else { Some(n as i64) });
    sqlx::query(
        "INSERT INTO agent_slots \
         (agent_id, slot_index, name, provider, model, system_prompt, skill_ids_json, max_tokens, prompt_version, inputs_policy, bar_history_limit) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(agent_id)
    .bind(idx)
    .bind(&slot.name)
    .bind(&slot.provider)
    .bind(&slot.model)
    .bind(&slot.system_prompt)
    .bind(&skill_ids_json)
    // `None` persists as the sentinel `0`; `Some(0)` is also treated as
    // unset to keep round-trips stable.
    .bind(slot.max_tokens.unwrap_or(0) as i64)
    .bind(prompt_version)
    // F-6: persisted as one of `raw` | `causal` | `oracle`. The DB
    // column has DEFAULT 'raw' (migration 020), but we always bind
    // the explicit string here so the row is unambiguous and the
    // read-side roundtrip is byte-stable.
    .bind(slot.inputs_policy.as_str())
    // F-8: explicit NULLable INTEGER. Operators leaving it unset get
    // SQL NULL → `None` on read, preserving pre-022 behavior.
    .bind(bar_history_limit_db)
    .execute(&mut **tx)
    .await
    .with_context(|| format!("insert slot {} for agent {}", slot.name, agent_id))?;
    Ok(())
}

fn row_to_agent(row: sqlx::sqlite::SqliteRow, slots: Vec<AgentSlot>) -> Result<Agent> {
    let tags_json: String = row.try_get("tags_json")?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).context("parse tags_json")?;
    let archived_int: i64 = row.try_get("archived")?;
    let created_at_s: String = row.try_get("created_at")?;
    let updated_at_s: String = row.try_get("updated_at")?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_s)?.with_timezone(&Utc);
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_s)?.with_timezone(&Utc);

    Ok(Agent {
        agent_id: row.try_get("agent_id")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        tags,
        slots,
        archived: archived_int != 0,
        created_at,
        updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fresh_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        // 005 creates the agents + agent_slots tables.
        let migration_005 = include_str!("../../migrations/005_agents.sql");
        sqlx::query(migration_005).execute(&pool).await.unwrap();
        // 019 adds agent_slots.prompt_version, which AgentStore::insert_slot
        // writes on every save. Without it, every test that creates an
        // agent fails on insert.
        let migration_019 = include_str!("../../migrations/019_agent_slot_prompt_version.sql");
        sqlx::query(migration_019).execute(&pool).await.unwrap();
        // 020 adds agent_slots.inputs_policy (F-6 causal sanitization).
        let migration_020 = include_str!("../../migrations/020_agent_slot_inputs_policy.sql");
        sqlx::query(migration_020).execute(&pool).await.unwrap();
        // 023 adds agent_slots.bar_history_limit (F-8 rolling-window
        // cap + provider prompt cache). AgentStore::insert_slot now
        // writes this column on every save.
        let migration_023 = include_str!("../../migrations/023_agent_slot_cache_and_window.sql");
        sqlx::query(migration_023).execute(&pool).await.unwrap();
        pool
    }

    fn sample_slot() -> AgentSlot {
        // Prompt is intentionally ≥200 chars and does not start with the
        // default-placeholder text so the save-gate checks pass.
        let system_prompt = "You are a quantitative trading assistant. Analyse the OHLCV data \
                             provided and respond with a JSON object containing: action \
                             (buy/sell/hold), size_pct (0–100), and reason (string). \
                             Apply disciplined risk management: never risk more than 1% of \
                             notional equity per trade, and always respect the configured \
                             stop-loss and take-profit levels. Avoid over-trading on low-volume bars."
            .to_string();
        AgentSlot {
            name: "main".to_string(),
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            system_prompt,
            skill_ids: vec![],
            max_tokens: Some(4096),
            temperature: None,
            prompt_version: String::new(),
            inputs_policy: InputsPolicy::Raw,
            bar_history_limit: None,
        }
    }

    #[tokio::test]
    async fn create_then_get_round_trips() {
        let store = AgentStore::new(fresh_pool().await);
        // Name uses no asset slug so the name↔prompt mismatch check does not
        // fire; the test is purely about DB round-trip fidelity.
        let id = store
            .create(NewAgent {
                name: "mean-rev-v1".to_string(),
                description: "Buys dips on 15m.".to_string(),
                tags: vec!["mean-rev".to_string(), "btc".to_string()],
                slots: vec![sample_slot()],
            })
            .await
            .unwrap();

        let loaded = store.get(&id).await.unwrap().expect("exists");
        assert_eq!(loaded.name, "mean-rev-v1");
        assert_eq!(loaded.tags, vec!["mean-rev", "btc"]);
        assert_eq!(loaded.slots.len(), 1);
        assert_eq!(loaded.slots[0].name, "main");
        assert!(!loaded.archived);
    }

    #[tokio::test]
    async fn list_excludes_archived_by_default() {
        let store = AgentStore::new(fresh_pool().await);
        let a = store
            .create(NewAgent {
                name: "a".to_string(),
                description: String::new(),
                tags: vec![],
                slots: vec![sample_slot()],
            })
            .await
            .unwrap();
        let _b = store
            .create(NewAgent {
                name: "b".to_string(),
                description: String::new(),
                tags: vec![],
                slots: vec![sample_slot()],
            })
            .await
            .unwrap();
        assert!(store.archive(&a).await.unwrap());

        let active = store.list(ListFilter::default()).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "b");

        let all = store
            .list(ListFilter {
                include_archived: true,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn update_replaces_slots() {
        let store = AgentStore::new(fresh_pool().await);
        let id = store
            .create(NewAgent {
                name: "z".to_string(),
                description: String::new(),
                tags: vec![],
                slots: vec![sample_slot()],
            })
            .await
            .unwrap();

        let two_slots = vec![
            AgentSlot {
                name: "trader".to_string(),
                ..sample_slot()
            },
            AgentSlot {
                name: "risk_check".to_string(),
                ..sample_slot()
            },
        ];
        let updated = store
            .update(
                &id,
                UpdateAgent {
                    slots: Some(two_slots),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .expect("exists");
        assert_eq!(updated.slots.len(), 2);
        assert_eq!(updated.slots[0].name, "trader");
        assert_eq!(updated.slots[1].name, "risk_check");
    }

    #[tokio::test]
    async fn name_exists_uniqueness_check() {
        let store = AgentStore::new(fresh_pool().await);
        let id = store
            .create(NewAgent {
                name: "taken".to_string(),
                description: String::new(),
                tags: vec![],
                slots: vec![sample_slot()],
            })
            .await
            .unwrap();

        assert!(store.name_exists("taken", None).await.unwrap());
        assert!(!store.name_exists("free", None).await.unwrap());
        // Same id excluded — should report not-taken so the owner can save without conflict.
        assert!(!store.name_exists("taken", Some(&id)).await.unwrap());
    }

    #[tokio::test]
    async fn get_returns_none_for_missing() {
        let store = AgentStore::new(fresh_pool().await);
        assert!(store.get("01HZ000000000000000000XXXX").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn none_max_tokens_round_trips_as_unset() {
        let store = AgentStore::new(fresh_pool().await);
        let id = store
            .create(NewAgent {
                name: "auto-tokens".to_string(),
                description: String::new(),
                tags: vec![],
                slots: vec![AgentSlot {
                    max_tokens: None,
                    ..sample_slot()
                }],
            })
            .await
            .unwrap();
        let loaded = store.get(&id).await.unwrap().expect("exists");
        assert_eq!(loaded.slots[0].max_tokens, None);
    }

    #[tokio::test]
    async fn inputs_policy_round_trips_through_create_and_update() {
        // F-6: AgentStore must round-trip the three policy values
        // through both `create` and `update`. This is the wire-level
        // half of the contract; the executor's policy-aware
        // serialization is pinned in `tests/eval_executor_paper.rs`.
        let store = AgentStore::new(fresh_pool().await);
        for policy in [InputsPolicy::Raw, InputsPolicy::Causal, InputsPolicy::Oracle] {
            let id = store
                .create(NewAgent {
                    name: format!("policy-{}", policy.as_str()),
                    description: String::new(),
                    tags: vec![],
                    slots: vec![AgentSlot {
                        inputs_policy: policy,
                        ..sample_slot()
                    }],
                })
                .await
                .unwrap();
            let loaded = store.get(&id).await.unwrap().expect("exists");
            assert_eq!(
                loaded.slots[0].inputs_policy, policy,
                "create round-trip failed for {policy:?}",
            );
        }

        // Update path: flip a Raw slot to Causal, confirm the column
        // moves with it.
        let id = store
            .create(NewAgent {
                name: "flip-me".to_string(),
                description: String::new(),
                tags: vec![],
                slots: vec![sample_slot()], // Raw default
            })
            .await
            .unwrap();
        let loaded = store.get(&id).await.unwrap().expect("exists");
        assert_eq!(loaded.slots[0].inputs_policy, InputsPolicy::Raw);
        let updated = store
            .update(
                &id,
                UpdateAgent {
                    slots: Some(vec![AgentSlot {
                        inputs_policy: InputsPolicy::Causal,
                        ..sample_slot()
                    }]),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .expect("exists");
        assert_eq!(updated.slots[0].inputs_policy, InputsPolicy::Causal);
    }

    #[tokio::test]
    async fn bar_history_limit_round_trips_through_create_and_update() {
        // F-8: AgentStore must round-trip the optional cap through both
        // `create` and `update`. The default (NULL) preserves today's
        // behavior; `Some(50)` is the canonical "stable prefix" value
        // the prompt-cache wave uses.
        let store = AgentStore::new(fresh_pool().await);

        // None round-trips as NULL → None.
        let id = store
            .create(NewAgent {
                name: "no-cap".into(),
                description: String::new(),
                tags: vec![],
                slots: vec![AgentSlot {
                    bar_history_limit: None,
                    ..sample_slot()
                }],
            })
            .await
            .unwrap();
        let loaded = store.get(&id).await.unwrap().expect("exists");
        assert_eq!(loaded.slots[0].bar_history_limit, None);

        // Some(50) round-trips verbatim.
        let id = store
            .create(NewAgent {
                name: "cap-50".into(),
                description: String::new(),
                tags: vec![],
                slots: vec![AgentSlot {
                    bar_history_limit: Some(50),
                    ..sample_slot()
                }],
            })
            .await
            .unwrap();
        let loaded = store.get(&id).await.unwrap().expect("exists");
        assert_eq!(loaded.slots[0].bar_history_limit, Some(50));

        // Some(0) is normalised to None — defence against a stray zero
        // dropping the trader's view entirely.
        let id = store
            .create(NewAgent {
                name: "stray-zero".into(),
                description: String::new(),
                tags: vec![],
                slots: vec![AgentSlot {
                    bar_history_limit: Some(0),
                    ..sample_slot()
                }],
            })
            .await
            .unwrap();
        let loaded = store.get(&id).await.unwrap().expect("exists");
        assert_eq!(loaded.slots[0].bar_history_limit, None);

        // Update path: flip from None to Some(10).
        let id = store
            .create(NewAgent {
                name: "flip-cap".into(),
                description: String::new(),
                tags: vec![],
                slots: vec![sample_slot()],
            })
            .await
            .unwrap();
        let updated = store
            .update(
                &id,
                UpdateAgent {
                    slots: Some(vec![AgentSlot {
                        bar_history_limit: Some(10),
                        ..sample_slot()
                    }]),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .expect("exists");
        assert_eq!(updated.slots[0].bar_history_limit, Some(10));
    }

    #[tokio::test]
    async fn explicit_max_tokens_round_trips() {
        let store = AgentStore::new(fresh_pool().await);
        let id = store
            .create(NewAgent {
                name: "manual-tokens".to_string(),
                description: String::new(),
                tags: vec![],
                slots: vec![AgentSlot {
                    max_tokens: Some(6000),
                    ..sample_slot()
                }],
            })
            .await
            .unwrap();
        let loaded = store.get(&id).await.unwrap().expect("exists");
        assert_eq!(loaded.slots[0].max_tokens, Some(6000));
    }
}
