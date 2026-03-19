use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KnowledgeBaseRow {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub name: String,
    pub description: String,
    pub embedding_model: String,
    pub kb_type: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KnowledgeBaseWithCount {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub name: String,
    pub description: String,
    pub embedding_model: String,
    pub kb_type: String,
    pub entry_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Agent memory KB with the agent's name joined in.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AgentMemoryKb {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub agent_name: String,
    pub name: String,
    pub description: String,
    pub embedding_model: String,
    pub kb_type: String,
    pub entry_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn create(
    pool: &PgPool,
    owner_id: Uuid,
    name: &str,
    description: &str,
    embedding_model: &str,
) -> Result<KnowledgeBaseRow, sqlx::Error> {
    sqlx::query_as::<_, KnowledgeBaseRow>(
        r#"
        INSERT INTO knowledge_bases (owner_id, name, description, embedding_model)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(owner_id)
    .bind(name)
    .bind(description)
    .bind(embedding_model)
    .fetch_one(pool)
    .await
}

/// Create a knowledge base owned by an agent and auto-link it.
pub async fn create_for_agent(
    pool: &PgPool,
    agent_id: Uuid,
    owner_id: Uuid,
    name: &str,
    description: &str,
    embedding_model: &str,
) -> Result<KnowledgeBaseRow, sqlx::Error> {
    let row = sqlx::query_as::<_, KnowledgeBaseRow>(
        r#"
        INSERT INTO knowledge_bases (owner_id, agent_id, name, description, embedding_model)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING *
        "#,
    )
    .bind(owner_id)
    .bind(agent_id)
    .bind(name)
    .bind(description)
    .bind(embedding_model)
    .fetch_one(pool)
    .await?;

    // Auto-link to the agent
    agent_link(pool, agent_id, row.id).await?;

    Ok(row)
}

pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Option<KnowledgeBaseWithCount>, sqlx::Error> {
    sqlx::query_as::<_, KnowledgeBaseWithCount>(
        r#"
        SELECT kb.*, COALESCE(c.cnt, 0) AS entry_count
        FROM knowledge_bases kb
        LEFT JOIN (SELECT knowledge_base_id, COUNT(*) AS cnt FROM knowledge_entries GROUP BY knowledge_base_id) c
            ON c.knowledge_base_id = kb.id
        WHERE kb.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// List knowledge bases owned by or shared with a user.
pub async fn list_for_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<KnowledgeBaseWithCount>, sqlx::Error> {
    sqlx::query_as::<_, KnowledgeBaseWithCount>(
        r#"
        SELECT kb.*, COALESCE(c.cnt, 0) AS entry_count
        FROM knowledge_bases kb
        LEFT JOIN (SELECT knowledge_base_id, COUNT(*) AS cnt FROM knowledge_entries GROUP BY knowledge_base_id) c
            ON c.knowledge_base_id = kb.id
        WHERE kb.kb_type = 'standard'
          AND (kb.owner_id = $1
               OR kb.id IN (SELECT knowledge_base_id FROM knowledge_base_shares WHERE shared_with = $1))
        ORDER BY kb.updated_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// List all knowledge bases (admin).
pub async fn list_all(pool: &PgPool) -> Result<Vec<KnowledgeBaseWithCount>, sqlx::Error> {
    sqlx::query_as::<_, KnowledgeBaseWithCount>(
        r#"
        SELECT kb.*, COALESCE(c.cnt, 0) AS entry_count
        FROM knowledge_bases kb
        LEFT JOIN (SELECT knowledge_base_id, COUNT(*) AS cnt FROM knowledge_entries GROUP BY knowledge_base_id) c
            ON c.knowledge_base_id = kb.id
        WHERE kb.kb_type = 'standard'
        ORDER BY kb.updated_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn update(
    pool: &PgPool,
    id: Uuid,
    name: &str,
    description: &str,
) -> Result<Option<KnowledgeBaseRow>, sqlx::Error> {
    sqlx::query_as::<_, KnowledgeBaseRow>(
        "UPDATE knowledge_bases SET name = $2, description = $3, updated_at = now() WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(name)
    .bind(description)
    .fetch_optional(pool)
    .await
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM knowledge_bases WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Get or create the per-agent "Memory" knowledge base.
pub async fn get_or_create_agent_memory_kb(
    pool: &PgPool,
    agent_id: Uuid,
    owner_id: Uuid,
    embedding_model: &str,
) -> Result<KnowledgeBaseRow, sqlx::Error> {
    // Try insert first, ignore conflict (unique index on agent_id WHERE kb_type='memory')
    let _ = sqlx::query(
        r#"
        INSERT INTO knowledge_bases (owner_id, agent_id, name, description, kb_type, embedding_model)
        VALUES ($1, $2, 'Memory', 'Auto-embedded conversation history', 'memory', $3)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(owner_id)
    .bind(agent_id)
    .bind(embedding_model)
    .execute(pool)
    .await;

    // Now fetch it
    sqlx::query_as::<_, KnowledgeBaseRow>(
        "SELECT * FROM knowledge_bases WHERE agent_id = $1 AND kb_type = 'memory'",
    )
    .bind(agent_id)
    .fetch_one(pool)
    .await
}

/// List all agent memory KBs for agents owned by (or shared with) a user.
pub async fn list_agent_memory_kbs(pool: &PgPool, user_id: Uuid) -> Result<Vec<AgentMemoryKb>, sqlx::Error> {
    sqlx::query_as::<_, AgentMemoryKb>(
        r#"
        SELECT kb.*, a.name AS agent_name, COALESCE(c.cnt, 0) AS entry_count
        FROM knowledge_bases kb
        INNER JOIN agents a ON a.id = kb.agent_id
        LEFT JOIN (SELECT knowledge_base_id, COUNT(*) AS cnt FROM knowledge_entries GROUP BY knowledge_base_id) c
            ON c.knowledge_base_id = kb.id
        WHERE kb.kb_type = 'memory'
          AND (a.owner_id = $1 OR a.shared = true)
        ORDER BY a.name
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// Check if a knowledge base is a memory type (cannot be deleted by user).
pub async fn is_memory_kb(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT kb_type FROM knowledge_bases WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|r| r.0 == "memory").unwrap_or(false))
}

/// Check if a knowledge base is linked to a specific agent.
pub async fn is_linked_to_agent(pool: &PgPool, kb_id: Uuid, agent_id: Uuid) -> Result<bool, sqlx::Error> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT agent_id FROM agent_knowledge_bases WHERE knowledge_base_id = $1 AND agent_id = $2",
    )
    .bind(kb_id)
    .bind(agent_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

// ── Sharing ────────────────────────────────────────────────────────

use crate::share::SharePermission;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KbShareRow {
    pub id: Uuid,
    pub knowledge_base_id: Uuid,
    pub shared_by: Uuid,
    pub shared_with: Uuid,
    pub permission: SharePermission,
    pub created_at: DateTime<Utc>,
}

pub async fn share_create(
    pool: &PgPool,
    knowledge_base_id: Uuid,
    shared_by: Uuid,
    shared_with: Uuid,
    permission: SharePermission,
) -> Result<KbShareRow, sqlx::Error> {
    sqlx::query_as::<_, KbShareRow>(
        r#"
        INSERT INTO knowledge_base_shares (knowledge_base_id, shared_by, shared_with, permission)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (knowledge_base_id, shared_with) DO UPDATE SET permission = $4
        RETURNING *
        "#,
    )
    .bind(knowledge_base_id)
    .bind(shared_by)
    .bind(shared_with)
    .bind(permission)
    .fetch_one(pool)
    .await
}

pub async fn share_list(pool: &PgPool, knowledge_base_id: Uuid) -> Result<Vec<KbShareRow>, sqlx::Error> {
    sqlx::query_as::<_, KbShareRow>(
        "SELECT * FROM knowledge_base_shares WHERE knowledge_base_id = $1 ORDER BY created_at",
    )
    .bind(knowledge_base_id)
    .fetch_all(pool)
    .await
}

pub async fn share_delete(pool: &PgPool, knowledge_base_id: Uuid, shared_with: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM knowledge_base_shares WHERE knowledge_base_id = $1 AND shared_with = $2",
    )
    .bind(knowledge_base_id)
    .bind(shared_with)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn user_has_access(pool: &PgPool, knowledge_base_id: Uuid, user_id: Uuid) -> Result<bool, sqlx::Error> {
    let row = sqlx::query_as::<_, KbShareRow>(
        "SELECT * FROM knowledge_base_shares WHERE knowledge_base_id = $1 AND shared_with = $2",
    )
    .bind(knowledge_base_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

// ── Agent access ───────────────────────────────────────────────────

pub async fn agent_link(pool: &PgPool, agent_id: Uuid, knowledge_base_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO agent_knowledge_bases (agent_id, knowledge_base_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(agent_id)
    .bind(knowledge_base_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn agent_unlink(pool: &PgPool, agent_id: Uuid, knowledge_base_id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM agent_knowledge_bases WHERE agent_id = $1 AND knowledge_base_id = $2",
    )
    .bind(agent_id)
    .bind(knowledge_base_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn agent_list_kbs(pool: &PgPool, agent_id: Uuid) -> Result<Vec<KnowledgeBaseWithCount>, sqlx::Error> {
    sqlx::query_as::<_, KnowledgeBaseWithCount>(
        r#"
        SELECT kb.*, COALESCE(c.cnt, 0) AS entry_count
        FROM knowledge_bases kb
        INNER JOIN agent_knowledge_bases akb ON akb.knowledge_base_id = kb.id
        LEFT JOIN (SELECT knowledge_base_id, COUNT(*) AS cnt FROM knowledge_entries GROUP BY knowledge_base_id) c
            ON c.knowledge_base_id = kb.id
        WHERE akb.agent_id = $1
        ORDER BY kb.name
        "#,
    )
    .bind(agent_id)
    .fetch_all(pool)
    .await
}

/// List agent IDs linked to a knowledge base.
pub async fn kb_list_agents(pool: &PgPool, knowledge_base_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT agent_id FROM agent_knowledge_bases WHERE knowledge_base_id = $1",
    )
    .bind(knowledge_base_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}
