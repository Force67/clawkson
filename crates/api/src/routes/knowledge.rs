use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use chrono::Utc;
use clawkson_core::KnowledgeEntry;
use serde::Deserialize;
use uuid::Uuid;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_entries).post(create_entry))
        .route("/{id}", get(get_entry))
}

#[derive(Debug, Deserialize)]
pub struct CreateKnowledgeEntryRequest {
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
}

async fn list_entries(State(state): State<AppState>) -> Json<Vec<KnowledgeEntry>> {
    let inner = state.inner.read().await;
    Json(inner.knowledge.clone())
}

async fn get_entry(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Json<Option<KnowledgeEntry>> {
    let inner = state.inner.read().await;
    let entry = inner.knowledge.iter().find(|k| k.id == id).cloned();
    Json(entry)
}

async fn create_entry(
    State(state): State<AppState>,
    Json(req): Json<CreateKnowledgeEntryRequest>,
) -> Json<KnowledgeEntry> {
    let now = Utc::now();
    let entry = KnowledgeEntry {
        id: Uuid::new_v4(),
        title: req.title,
        content: req.content,
        tags: req.tags,
        created_at: now,
        updated_at: now,
    };

    let mut inner = state.inner.write().await;
    inner.knowledge.push(entry.clone());
    Json(entry)
}
