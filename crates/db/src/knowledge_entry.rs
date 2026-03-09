use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KnowledgeEntryRow {
    pub id: Uuid,
    pub knowledge_base_id: Uuid,
    pub title: String,
    pub content: String,
    pub token_count: Option<i32>,
    pub has_embedding: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn create(
    pool: &PgPool,
    knowledge_base_id: Uuid,
    title: &str,
    content: &str,
) -> Result<KnowledgeEntryRow, sqlx::Error> {
    sqlx::query_as::<_, KnowledgeEntryRow>(
        r#"
        INSERT INTO knowledge_entries (knowledge_base_id, title, content)
        VALUES ($1, $2, $3)
        RETURNING id, knowledge_base_id, title, content, token_count,
                  (embedding IS NOT NULL) AS has_embedding, created_at, updated_at
        "#,
    )
    .bind(knowledge_base_id)
    .bind(title)
    .bind(content)
    .fetch_one(pool)
    .await
}

pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Option<KnowledgeEntryRow>, sqlx::Error> {
    sqlx::query_as::<_, KnowledgeEntryRow>(
        r#"
        SELECT id, knowledge_base_id, title, content, token_count,
               (embedding IS NOT NULL) AS has_embedding, created_at, updated_at
        FROM knowledge_entries WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn list_for_kb(pool: &PgPool, knowledge_base_id: Uuid) -> Result<Vec<KnowledgeEntryRow>, sqlx::Error> {
    sqlx::query_as::<_, KnowledgeEntryRow>(
        r#"
        SELECT id, knowledge_base_id, title, content, token_count,
               (embedding IS NOT NULL) AS has_embedding, created_at, updated_at
        FROM knowledge_entries WHERE knowledge_base_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(knowledge_base_id)
    .fetch_all(pool)
    .await
}

pub async fn update(
    pool: &PgPool,
    id: Uuid,
    title: &str,
    content: &str,
) -> Result<Option<KnowledgeEntryRow>, sqlx::Error> {
    // Clear embedding when content changes — needs re-embedding
    sqlx::query_as::<_, KnowledgeEntryRow>(
        r#"
        UPDATE knowledge_entries
        SET title = $2, content = $3, embedding = NULL, token_count = NULL, updated_at = now()
        WHERE id = $1
        RETURNING id, knowledge_base_id, title, content, token_count,
                  (embedding IS NOT NULL) AS has_embedding, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(title)
    .bind(content)
    .fetch_optional(pool)
    .await
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM knowledge_entries WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Store an embedding vector for an entry.
pub async fn set_embedding(
    pool: &PgPool,
    id: Uuid,
    embedding: &[f32],
    token_count: Option<i32>,
) -> Result<(), sqlx::Error> {
    // pgvector expects a text representation like '[0.1, 0.2, ...]'
    let vec_str = format!(
        "[{}]",
        embedding.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(",")
    );
    sqlx::query(
        r#"
        UPDATE knowledge_entries
        SET embedding = $2::vector, token_count = $3, updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(vec_str)
    .bind(token_count)
    .execute(pool)
    .await?;
    Ok(())
}

/// Entries in a knowledge base that are missing embeddings.
pub async fn list_without_embedding(pool: &PgPool, knowledge_base_id: Uuid) -> Result<Vec<KnowledgeEntryRow>, sqlx::Error> {
    sqlx::query_as::<_, KnowledgeEntryRow>(
        r#"
        SELECT id, knowledge_base_id, title, content, token_count,
               (embedding IS NOT NULL) AS has_embedding, created_at, updated_at
        FROM knowledge_entries
        WHERE knowledge_base_id = $1 AND embedding IS NULL
        ORDER BY created_at
        "#,
    )
    .bind(knowledge_base_id)
    .fetch_all(pool)
    .await
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SearchResult {
    pub id: Uuid,
    pub knowledge_base_id: Uuid,
    pub title: String,
    pub content: String,
    pub token_count: Option<i32>,
    pub score: f64,
}

/// Vector similarity search across one or more knowledge bases.
pub async fn search(
    pool: &PgPool,
    knowledge_base_ids: &[Uuid],
    query_embedding: &[f32],
    limit: i64,
) -> Result<Vec<SearchResult>, sqlx::Error> {
    let vec_str = format!(
        "[{}]",
        query_embedding.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(",")
    );
    sqlx::query_as::<_, SearchResult>(
        r#"
        SELECT id, knowledge_base_id, title, content, token_count,
               1 - (embedding <=> $2::vector) AS score
        FROM knowledge_entries
        WHERE knowledge_base_id = ANY($1) AND embedding IS NOT NULL
        ORDER BY embedding <=> $2::vector
        LIMIT $3
        "#,
    )
    .bind(knowledge_base_ids)
    .bind(vec_str)
    .bind(limit)
    .fetch_all(pool)
    .await
}
