use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize)]
pub struct SkillRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn create(
    db: &Db,
    name: &str,
    description: &str,
    instructions: &str,
) -> Result<SkillRow, DbError> {
    let row = sqlx::query_as::<_, SkillRow>(
        "INSERT INTO skills (name, description, instructions)
         VALUES ($1, $2, $3)
         RETURNING *",
    )
    .bind(name)
    .bind(description)
    .bind(instructions)
    .fetch_one(db.pool())
    .await?;

    Ok(row)
}

pub async fn get_by_id(db: &Db, id: Uuid) -> Result<Option<SkillRow>, DbError> {
    let row = sqlx::query_as::<_, SkillRow>("SELECT * FROM skills WHERE id = $1")
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

    Ok(row)
}

pub async fn get_by_name(db: &Db, name: &str) -> Result<Option<SkillRow>, DbError> {
    let row = sqlx::query_as::<_, SkillRow>("SELECT * FROM skills WHERE name = $1")
        .bind(name)
        .fetch_optional(db.pool())
        .await?;

    Ok(row)
}

pub async fn list_all(db: &Db) -> Result<Vec<SkillRow>, DbError> {
    let rows = sqlx::query_as::<_, SkillRow>(
        "SELECT * FROM skills ORDER BY name ASC",
    )
    .fetch_all(db.pool())
    .await?;

    Ok(rows)
}

pub async fn update(
    db: &Db,
    id: Uuid,
    name: Option<&str>,
    description: Option<&str>,
    instructions: Option<&str>,
) -> Result<Option<SkillRow>, DbError> {
    let Some(mut skill) = get_by_id(db, id).await? else {
        return Ok(None);
    };

    if let Some(v) = name { skill.name = v.to_string(); }
    if let Some(v) = description { skill.description = v.to_string(); }
    if let Some(v) = instructions { skill.instructions = v.to_string(); }

    let row = sqlx::query_as::<_, SkillRow>(
        "UPDATE skills
         SET name = $2, description = $3, instructions = $4, updated_at = now()
         WHERE id = $1
         RETURNING *",
    )
    .bind(id)
    .bind(&skill.name)
    .bind(&skill.description)
    .bind(&skill.instructions)
    .fetch_optional(db.pool())
    .await?;

    Ok(row)
}

pub async fn delete(db: &Db, id: Uuid) -> Result<bool, DbError> {
    let result = sqlx::query("DELETE FROM skills WHERE id = $1")
        .bind(id)
        .execute(db.pool())
        .await?;

    Ok(result.rows_affected() > 0)
}

// ── Agent ↔ Skill linking ────────────────────────────────────────

pub async fn agent_link(pool: &PgPool, agent_id: Uuid, skill_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO agent_skills (agent_id, skill_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(agent_id)
    .bind(skill_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn agent_unlink(pool: &PgPool, agent_id: Uuid, skill_id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM agent_skills WHERE agent_id = $1 AND skill_id = $2",
    )
    .bind(agent_id)
    .bind(skill_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// List all skills linked to a given agent.
pub async fn agent_list_skills(pool: &PgPool, agent_id: Uuid) -> Result<Vec<SkillRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, SkillRow>(
        "SELECT s.* FROM skills s
         JOIN agent_skills a ON a.skill_id = s.id
         WHERE a.agent_id = $1
         ORDER BY s.name ASC",
    )
    .bind(agent_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// List agent IDs linked to a given skill.
pub async fn skill_list_agents(pool: &PgPool, skill_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows = sqlx::query_scalar::<_, Uuid>(
        "SELECT agent_id FROM agent_skills WHERE skill_id = $1",
    )
    .bind(skill_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}
