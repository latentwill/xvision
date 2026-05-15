//! `SkillStore` — sqlx-backed CRUD for skills.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use ulid::Ulid;

use crate::skills::model::{Skill, SkillKind};

#[derive(Debug, Clone)]
pub struct SkillStore {
    pool: SqlitePool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewSkill {
    pub name: String,
    pub description: String,
    pub kind: SkillKind,
    pub config: serde_json::Value,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateSkill {
    pub name: Option<String>,
    pub description: Option<String>,
    pub kind: Option<SkillKind>,
    pub config: Option<serde_json::Value>,
}

impl SkillStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, new: NewSkill) -> Result<String> {
        let id = Ulid::new().to_string();
        let now = Utc::now().to_rfc3339();
        let config_str = serde_json::to_string(&new.config).context("serialize config")?;

        sqlx::query(
            "INSERT INTO skills (skill_id, name, description, kind, config_json, archived, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, 0, ?, ?)",
        )
        .bind(&id)
        .bind(&new.name)
        .bind(&new.description)
        .bind(new.kind.as_str())
        .bind(&config_str)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .with_context(|| format!("insert skill name={}", new.name))?;

        Ok(id)
    }

    pub async fn get(&self, skill_id: &str) -> Result<Option<Skill>> {
        let row = sqlx::query(
            "SELECT skill_id, name, description, kind, config_json, archived, created_at, updated_at \
             FROM skills WHERE skill_id = ?",
        )
        .bind(skill_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else { return Ok(None) };
        Ok(Some(row_to_skill(row)?))
    }

    pub async fn list(&self, include_archived: bool) -> Result<Vec<Skill>> {
        let sql = if include_archived {
            "SELECT skill_id, name, description, kind, config_json, archived, created_at, updated_at \
             FROM skills ORDER BY updated_at DESC"
        } else {
            "SELECT skill_id, name, description, kind, config_json, archived, created_at, updated_at \
             FROM skills WHERE archived = 0 ORDER BY updated_at DESC"
        };

        let rows = sqlx::query(sql).fetch_all(&self.pool).await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(row_to_skill(row)?);
        }
        Ok(out)
    }

    pub async fn update(&self, skill_id: &str, patch: UpdateSkill) -> Result<Option<Skill>> {
        let existing = self.get(skill_id).await?;
        if existing.is_none() {
            return Ok(None);
        }

        let now = Utc::now().to_rfc3339();

        if let Some(name) = patch.name {
            sqlx::query("UPDATE skills SET name = ?, updated_at = ? WHERE skill_id = ?")
                .bind(name)
                .bind(&now)
                .bind(skill_id)
                .execute(&self.pool)
                .await?;
        }
        if let Some(desc) = patch.description {
            sqlx::query("UPDATE skills SET description = ?, updated_at = ? WHERE skill_id = ?")
                .bind(desc)
                .bind(&now)
                .bind(skill_id)
                .execute(&self.pool)
                .await?;
        }
        if let Some(kind) = patch.kind {
            sqlx::query("UPDATE skills SET kind = ?, updated_at = ? WHERE skill_id = ?")
                .bind(kind.as_str())
                .bind(&now)
                .bind(skill_id)
                .execute(&self.pool)
                .await?;
        }
        if let Some(config) = patch.config {
            let s = serde_json::to_string(&config).context("serialize config")?;
            sqlx::query("UPDATE skills SET config_json = ?, updated_at = ? WHERE skill_id = ?")
                .bind(s)
                .bind(&now)
                .bind(skill_id)
                .execute(&self.pool)
                .await?;
        }

        self.get(skill_id).await
    }

    pub async fn archive(&self, skill_id: &str) -> Result<bool> {
        let now = Utc::now().to_rfc3339();
        let result =
            sqlx::query("UPDATE skills SET archived = 1, updated_at = ? WHERE skill_id = ? AND archived = 0")
                .bind(&now)
                .bind(skill_id)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn name_exists(&self, name: &str, excluding_id: Option<&str>) -> Result<bool> {
        let row = match excluding_id {
            Some(id) => {
                sqlx::query("SELECT 1 FROM skills WHERE name = ? AND skill_id != ? LIMIT 1")
                    .bind(name)
                    .bind(id)
                    .fetch_optional(&self.pool)
                    .await?
            }
            None => {
                sqlx::query("SELECT 1 FROM skills WHERE name = ? LIMIT 1")
                    .bind(name)
                    .fetch_optional(&self.pool)
                    .await?
            }
        };
        Ok(row.is_some())
    }
}

fn row_to_skill(row: sqlx::sqlite::SqliteRow) -> Result<Skill> {
    let kind_s: String = row.try_get("kind")?;
    let kind =
        SkillKind::parse(&kind_s).ok_or_else(|| anyhow::anyhow!("unknown skill kind in DB: {}", kind_s))?;
    let config_str: String = row.try_get("config_json")?;
    let config: serde_json::Value = serde_json::from_str(&config_str).context("parse config_json")?;
    let archived_int: i64 = row.try_get("archived")?;
    let created_s: String = row.try_get("created_at")?;
    let updated_s: String = row.try_get("updated_at")?;
    let created_at = DateTime::parse_from_rfc3339(&created_s)?.with_timezone(&Utc);
    let updated_at = DateTime::parse_from_rfc3339(&updated_s)?.with_timezone(&Utc);

    Ok(Skill {
        skill_id: row.try_get("skill_id")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        kind,
        config,
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
        let migration = include_str!("../../migrations/007_skills.sql");
        sqlx::query(migration).execute(&pool).await.unwrap();
        pool
    }

    fn sample_skill(name: &str) -> NewSkill {
        NewSkill {
            name: name.to_string(),
            description: "x".to_string(),
            kind: SkillKind::Tool,
            config: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn create_get_round_trip() {
        let s = SkillStore::new(fresh_pool().await);
        let id = s.create(sample_skill("test-skill")).await.unwrap();
        let loaded = s.get(&id).await.unwrap().expect("present");
        assert_eq!(loaded.name, "test-skill");
        assert_eq!(loaded.kind, SkillKind::Tool);
        assert!(!loaded.archived);
    }

    #[tokio::test]
    async fn list_excludes_archived_by_default() {
        let s = SkillStore::new(fresh_pool().await);
        let a = s.create(sample_skill("a")).await.unwrap();
        let _b = s.create(sample_skill("b")).await.unwrap();
        assert!(s.archive(&a).await.unwrap());

        let active = s.list(false).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "b");

        let all = s.list(true).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn update_changes_kind() {
        let s = SkillStore::new(fresh_pool().await);
        let id = s.create(sample_skill("k")).await.unwrap();
        let updated = s
            .update(
                &id,
                UpdateSkill {
                    kind: Some(SkillKind::PromptFragment),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .expect("exists");
        assert_eq!(updated.kind, SkillKind::PromptFragment);
    }

    #[tokio::test]
    async fn name_uniqueness_check() {
        let s = SkillStore::new(fresh_pool().await);
        let id = s.create(sample_skill("dup")).await.unwrap();
        assert!(s.name_exists("dup", None).await.unwrap());
        assert!(!s.name_exists("dup", Some(&id)).await.unwrap());
        assert!(!s.name_exists("nope", None).await.unwrap());
    }
}
