use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize)]
pub struct CredentialRow {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub description: String,
    pub credential_type: String,
    pub encrypted_value: String,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Summary returned for agent-linked credentials — never includes the value.
#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize)]
pub struct CredentialSummary {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub credential_type: String,
}

pub async fn create(
    db: &Db,
    owner_id: Uuid,
    name: &str,
    description: &str,
    credential_type: &str,
    encrypted_value: &str,
    metadata: &serde_json::Value,
) -> Result<CredentialRow, DbError> {
    let row = sqlx::query_as::<_, CredentialRow>(
        "INSERT INTO credentials (owner_id, name, description, credential_type, encrypted_value, metadata)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING *",
    )
    .bind(owner_id)
    .bind(name)
    .bind(description)
    .bind(credential_type)
    .bind(encrypted_value)
    .bind(metadata)
    .fetch_one(db.pool())
    .await?;

    Ok(row)
}

pub async fn get_by_id(db: &Db, id: Uuid) -> Result<Option<CredentialRow>, DbError> {
    let row = sqlx::query_as::<_, CredentialRow>("SELECT * FROM credentials WHERE id = $1")
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

    Ok(row)
}

pub async fn list_for_user(db: &Db, owner_id: Uuid) -> Result<Vec<CredentialRow>, DbError> {
    let rows = sqlx::query_as::<_, CredentialRow>(
        "SELECT * FROM credentials WHERE owner_id = $1 ORDER BY name ASC",
    )
    .bind(owner_id)
    .fetch_all(db.pool())
    .await?;

    Ok(rows)
}

pub async fn update(
    db: &Db,
    id: Uuid,
    name: Option<&str>,
    description: Option<&str>,
    credential_type: Option<&str>,
    encrypted_value: Option<&str>,
    metadata: Option<&serde_json::Value>,
) -> Result<Option<CredentialRow>, DbError> {
    let Some(mut cred) = get_by_id(db, id).await? else {
        return Ok(None);
    };

    if let Some(v) = name { cred.name = v.to_string(); }
    if let Some(v) = description { cred.description = v.to_string(); }
    if let Some(v) = credential_type { cred.credential_type = v.to_string(); }
    if let Some(v) = encrypted_value { cred.encrypted_value = v.to_string(); }
    if let Some(v) = metadata { cred.metadata = v.clone(); }

    let row = sqlx::query_as::<_, CredentialRow>(
        "UPDATE credentials
         SET name = $2, description = $3, credential_type = $4,
             encrypted_value = $5, metadata = $6, updated_at = now()
         WHERE id = $1
         RETURNING *",
    )
    .bind(id)
    .bind(&cred.name)
    .bind(&cred.description)
    .bind(&cred.credential_type)
    .bind(&cred.encrypted_value)
    .bind(&cred.metadata)
    .fetch_optional(db.pool())
    .await?;

    Ok(row)
}

pub async fn delete(db: &Db, id: Uuid) -> Result<bool, DbError> {
    let result = sqlx::query("DELETE FROM credentials WHERE id = $1")
        .bind(id)
        .execute(db.pool())
        .await?;

    Ok(result.rows_affected() > 0)
}

// ── Agent ↔ Credential linking ──────────────────────────────────

pub async fn agent_link(pool: &PgPool, agent_id: Uuid, credential_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO agent_credentials (agent_id, credential_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(agent_id)
    .bind(credential_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn agent_unlink(pool: &PgPool, agent_id: Uuid, credential_id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM agent_credentials WHERE agent_id = $1 AND credential_id = $2",
    )
    .bind(agent_id)
    .bind(credential_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// List credentials linked to an agent — returns summaries (name + type only, NO values).
pub async fn agent_list_credentials(pool: &PgPool, agent_id: Uuid) -> Result<Vec<CredentialSummary>, sqlx::Error> {
    let rows = sqlx::query_as::<_, CredentialSummary>(
        "SELECT c.id, c.name, c.description, c.credential_type FROM credentials c
         JOIN agent_credentials ac ON ac.credential_id = c.id
         WHERE ac.agent_id = $1
         ORDER BY c.name ASC",
    )
    .bind(agent_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Resolve a credential value for a given agent by credential name.
/// Used only at the tool execution layer — never expose this to LLM context.
pub async fn agent_resolve_credential(pool: &PgPool, agent_id: Uuid, credential_name: &str) -> Result<Option<CredentialRow>, sqlx::Error> {
    let row = sqlx::query_as::<_, CredentialRow>(
        "SELECT c.* FROM credentials c
         JOIN agent_credentials ac ON ac.credential_id = c.id
         WHERE ac.agent_id = $1 AND c.name = $2",
    )
    .bind(agent_id)
    .bind(credential_name)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Resolve all credential values for a given agent.
/// Used to inject env vars into containers — never expose to LLM context.
pub async fn agent_resolve_all_credentials(pool: &PgPool, agent_id: Uuid) -> Result<Vec<CredentialRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, CredentialRow>(
        "SELECT c.* FROM credentials c
         JOIN agent_credentials ac ON ac.credential_id = c.id
         WHERE ac.agent_id = $1
         ORDER BY c.name ASC",
    )
    .bind(agent_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// List agent IDs linked to a given credential.
pub async fn credential_list_agents(pool: &PgPool, credential_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows = sqlx::query_scalar::<_, Uuid>(
        "SELECT agent_id FROM agent_credentials WHERE credential_id = $1",
    )
    .bind(credential_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}
