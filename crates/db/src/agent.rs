use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "agent_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Online,
    Offline,
    Busy,
    Error,
}

#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize)]
pub struct AgentRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub status: AgentStatus,
    pub llm_connector_id: Option<Uuid>,
    pub system_prompt: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i32>,
    pub container_enabled: bool,
    pub container_config: Option<JsonValue>,
    pub owner_id: Option<Uuid>,
    pub shared: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn create(
    db: &Db,
    name: &str,
    description: &str,
    llm_connector_id: Option<Uuid>,
    system_prompt: Option<&str>,
    temperature: Option<f64>,
    max_tokens: Option<i32>,
    container_enabled: bool,
    container_config: Option<JsonValue>,
    owner_id: Uuid,
    shared: bool,
) -> Result<AgentRow, DbError> {
    let row = sqlx::query_as::<_, AgentRow>(
        "INSERT INTO agents (name, description, llm_connector_id, system_prompt, temperature, max_tokens, container_enabled, container_config, owner_id, shared)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING *",
    )
    .bind(name)
    .bind(description)
    .bind(llm_connector_id)
    .bind(system_prompt)
    .bind(temperature)
    .bind(max_tokens)
    .bind(container_enabled)
    .bind(container_config)
    .bind(owner_id)
    .bind(shared)
    .fetch_one(db.pool())
    .await?;

    Ok(row)
}

pub async fn get_by_id(db: &Db, id: Uuid) -> Result<Option<AgentRow>, DbError> {
    let row = sqlx::query_as::<_, AgentRow>(
        "SELECT * FROM agents WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db.pool())
    .await?;

    Ok(row)
}

/// List agents visible to a user: ones they own + shared agents.
/// Admins see all agents.
pub async fn list_for_user(db: &Db, user_id: Uuid, is_admin: bool) -> Result<Vec<AgentRow>, DbError> {
    if is_admin {
        let rows = sqlx::query_as::<_, AgentRow>(
            "SELECT * FROM agents ORDER BY created_at DESC",
        )
        .fetch_all(db.pool())
        .await?;
        return Ok(rows);
    }

    let rows = sqlx::query_as::<_, AgentRow>(
        "SELECT * FROM agents
         WHERE owner_id = $1 OR shared = TRUE
         ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(db.pool())
    .await?;

    Ok(rows)
}

pub async fn update(
    db: &Db,
    id: Uuid,
    name: Option<&str>,
    description: Option<&str>,
    status: Option<AgentStatus>,
    llm_connector_id: Option<Option<Uuid>>,
    system_prompt: Option<Option<&str>>,
    temperature: Option<Option<f64>>,
    max_tokens: Option<Option<i32>>,
    container_enabled: Option<bool>,
    container_config: Option<Option<JsonValue>>,
    shared: Option<bool>,
) -> Result<Option<AgentRow>, DbError> {
    // Fetch current then apply patches — simpler than dynamic SQL for this many optional fields
    let Some(mut agent) = get_by_id(db, id).await? else {
        return Ok(None);
    };

    if let Some(v) = name { agent.name = v.to_string(); }
    if let Some(v) = description { agent.description = v.to_string(); }
    if let Some(v) = status { agent.status = v; }
    if let Some(v) = llm_connector_id { agent.llm_connector_id = v; }
    if let Some(v) = system_prompt { agent.system_prompt = v.map(|s| s.to_string()); }
    if let Some(v) = temperature { agent.temperature = v; }
    if let Some(v) = max_tokens { agent.max_tokens = v; }
    if let Some(v) = container_enabled { agent.container_enabled = v; }
    if let Some(v) = container_config { agent.container_config = v; }
    if let Some(v) = shared { agent.shared = v; }

    let row = sqlx::query_as::<_, AgentRow>(
        "UPDATE agents
         SET name = $2, description = $3, status = $4, llm_connector_id = $5,
             system_prompt = $6, temperature = $7, max_tokens = $8,
             container_enabled = $9, container_config = $10, shared = $11, updated_at = now()
         WHERE id = $1
         RETURNING *",
    )
    .bind(id)
    .bind(&agent.name)
    .bind(&agent.description)
    .bind(agent.status)
    .bind(agent.llm_connector_id)
    .bind(&agent.system_prompt)
    .bind(agent.temperature)
    .bind(agent.max_tokens)
    .bind(agent.container_enabled)
    .bind(&agent.container_config)
    .bind(agent.shared)
    .fetch_optional(db.pool())
    .await?;

    Ok(row)
}

pub async fn delete(db: &Db, id: Uuid) -> Result<bool, DbError> {
    let result = sqlx::query("DELETE FROM agents WHERE id = $1")
        .bind(id)
        .execute(db.pool())
        .await?;

    Ok(result.rows_affected() > 0)
}
