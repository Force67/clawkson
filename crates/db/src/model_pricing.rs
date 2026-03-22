use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, FromRow)]
pub struct ModelPricingRow {
    pub id: Uuid,
    pub model: String,
    pub prompt_cost_per_million: f64,
    pub completion_cost_per_million: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// List all pricing entries.
pub async fn list(db: &Db) -> Result<Vec<ModelPricingRow>, DbError> {
    let rows = sqlx::query_as::<_, ModelPricingRow>(
        "SELECT id, model,
                prompt_cost_per_million::FLOAT8 AS prompt_cost_per_million,
                completion_cost_per_million::FLOAT8 AS completion_cost_per_million,
                created_at, updated_at
         FROM model_pricing
         ORDER BY model",
    )
    .fetch_all(db.pool())
    .await?;
    Ok(rows)
}

/// Insert or update pricing for a model.
pub async fn upsert(
    db: &Db,
    model: &str,
    prompt_cost_per_million: f64,
    completion_cost_per_million: f64,
) -> Result<ModelPricingRow, DbError> {
    let row = sqlx::query_as::<_, ModelPricingRow>(
        "INSERT INTO model_pricing (model, prompt_cost_per_million, completion_cost_per_million)
         VALUES ($1, $2::NUMERIC, $3::NUMERIC)
         ON CONFLICT (model) DO UPDATE SET
             prompt_cost_per_million = EXCLUDED.prompt_cost_per_million,
             completion_cost_per_million = EXCLUDED.completion_cost_per_million,
             updated_at = now()
         RETURNING id, model,
                   prompt_cost_per_million::FLOAT8 AS prompt_cost_per_million,
                   completion_cost_per_million::FLOAT8 AS completion_cost_per_million,
                   created_at, updated_at",
    )
    .bind(model)
    .bind(prompt_cost_per_million)
    .bind(completion_cost_per_million)
    .fetch_one(db.pool())
    .await?;
    Ok(row)
}

/// Delete a pricing entry.
pub async fn delete(db: &Db, id: Uuid) -> Result<bool, DbError> {
    let result = sqlx::query("DELETE FROM model_pricing WHERE id = $1")
        .bind(id)
        .execute(db.pool())
        .await?;
    Ok(result.rows_affected() > 0)
}
