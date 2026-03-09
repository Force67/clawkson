use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KnowledgeDocumentRow {
    pub id: Uuid,
    pub knowledge_base_id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub s3_key: String,
    pub size_bytes: i64,
    pub created_at: DateTime<Utc>,
}

pub async fn create(
    pool: &PgPool,
    id: Uuid,
    knowledge_base_id: Uuid,
    filename: &str,
    content_type: &str,
    s3_key: &str,
    size_bytes: i64,
) -> Result<KnowledgeDocumentRow, sqlx::Error> {
    sqlx::query_as::<_, KnowledgeDocumentRow>(
        r#"
        INSERT INTO knowledge_documents (id, knowledge_base_id, filename, content_type, s3_key, size_bytes)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, knowledge_base_id, filename, content_type, s3_key, size_bytes, created_at
        "#,
    )
    .bind(id)
    .bind(knowledge_base_id)
    .bind(filename)
    .bind(content_type)
    .bind(s3_key)
    .bind(size_bytes)
    .fetch_one(pool)
    .await
}

pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Option<KnowledgeDocumentRow>, sqlx::Error> {
    sqlx::query_as::<_, KnowledgeDocumentRow>(
        r#"
        SELECT id, knowledge_base_id, filename, content_type, s3_key, size_bytes, created_at
        FROM knowledge_documents WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn list_for_kb(pool: &PgPool, knowledge_base_id: Uuid) -> Result<Vec<KnowledgeDocumentRow>, sqlx::Error> {
    sqlx::query_as::<_, KnowledgeDocumentRow>(
        r#"
        SELECT id, knowledge_base_id, filename, content_type, s3_key, size_bytes, created_at
        FROM knowledge_documents WHERE knowledge_base_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(knowledge_base_id)
    .fetch_all(pool)
    .await
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM knowledge_documents WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
