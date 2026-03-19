use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, StatusCode},
    response::Response,
    routing::get,
    Json, Router,
};
use clawkson_core::{KnowledgeBase, KnowledgeDocument, KnowledgeEntry, KnowledgeSearchResult, MessageRole, SharePermission};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::embeddings::EmbeddingConfig;
use crate::state::AppState;

/// Load embedding provider config from app_settings.
async fn load_embedding_config(state: &AppState) -> EmbeddingConfig {
    match clawkson_db::settings::get(&state.db).await {
        Ok(row) => EmbeddingConfig {
            base_url: row.embedding_api_base_url,
            api_key: row.embedding_api_key,
            model: row.embedding_model,
        },
        Err(_) => EmbeddingConfig::default(),
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        // Knowledge bases
        .route("/", get(list_bases).post(create_base))
        .route("/agent-memories", get(list_agent_memories))
        .route("/{id}", get(get_base).patch(patch_base).delete(delete_base))
        // Entries within a base
        .route("/{id}/entries", get(list_entries).post(create_entry))
        .route("/{id}/upload", axum::routing::post(upload_files))
        .route("/{kb_id}/entries/{entry_id}", axum::routing::patch(patch_entry).delete(delete_entry))
        // Documents (original files in S3)
        .route("/{kb_id}/documents", get(list_documents))
        .route("/{kb_id}/documents/{doc_id}/download", get(download_document))
        .route("/{kb_id}/documents/{doc_id}", axum::routing::delete(delete_document))
        // Embedding generation
        .route("/{id}/embed", axum::routing::post(embed_entries))
        // Search
        .route("/{id}/search", axum::routing::post(search_entries))
        // Sharing
        .route("/{id}/shares", get(list_shares).post(create_share))
        .route("/{kb_id}/shares/{user_id}", axum::routing::delete(remove_share))
        // Agent access
        .route("/{id}/agents", get(list_agents).post(link_agent))
        .route("/{kb_id}/agents/{agent_id}", axum::routing::delete(unlink_agent))
}

// ── Request / Response types ──────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateKbRequest {
    pub name: String,
    pub description: Option<String>,
    pub embedding_model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PatchKbRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateEntryRequest {
    pub title: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct PatchEntryRequest {
    pub title: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ShareRequest {
    pub email: String,
    pub permission: SharePermission,
}

#[derive(Debug, Serialize)]
pub struct ShareInfo {
    pub id: Uuid,
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub permission: SharePermission,
}

#[derive(Debug, Deserialize)]
pub struct LinkAgentRequest {
    pub agent_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct EmbedResult {
    pub embedded: usize,
    pub failed: usize,
}

#[derive(Debug, Serialize)]
pub struct UploadResult {
    pub files_processed: usize,
    pub entries_created: usize,
    pub embedded: usize,
    pub embed_failed: usize,
    pub errors: Vec<String>,
}

// ── Helpers ───────────────────────────────────────────────────────

fn row_to_kb(row: &clawkson_db::knowledge_base::KnowledgeBaseWithCount) -> KnowledgeBase {
    KnowledgeBase {
        id: row.id,
        owner_id: row.owner_id,
        agent_id: row.agent_id,
        agent_name: None,
        name: row.name.clone(),
        description: row.description.clone(),
        kb_type: row.kb_type.clone(),
        embedding_model: row.embedding_model.clone(),
        entry_count: row.entry_count,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn row_to_entry(row: &clawkson_db::knowledge_entry::KnowledgeEntryRow) -> KnowledgeEntry {
    KnowledgeEntry {
        id: row.id,
        knowledge_base_id: row.knowledge_base_id,
        title: row.title.clone(),
        content: row.content.clone(),
        token_count: row.token_count,
        has_embedding: row.has_embedding,
        source_document_id: row.source_document_id,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn row_to_document(row: &clawkson_db::knowledge_document::KnowledgeDocumentRow) -> KnowledgeDocument {
    KnowledgeDocument {
        id: row.id,
        knowledge_base_id: row.knowledge_base_id,
        filename: row.filename.clone(),
        content_type: row.content_type.clone(),
        size_bytes: row.size_bytes,
        created_at: row.created_at,
    }
}

fn guess_content_type(filename: &str) -> &'static str {
    let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "csv" => "text/csv",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

/// Check that user owns or has share access to a knowledge base.
async fn check_access(pool: &clawkson_db::PgPool, kb_id: Uuid, user_id: Uuid, is_admin: bool) -> Result<bool, StatusCode> {
    let kb = clawkson_db::knowledge_base::get_by_id(pool, kb_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if is_admin || kb.owner_id == user_id {
        return Ok(true);
    }
    clawkson_db::knowledge_base::user_has_access(pool, kb_id, user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn check_owner(pool: &clawkson_db::PgPool, kb_id: Uuid, user_id: Uuid, is_admin: bool) -> Result<(), StatusCode> {
    let kb = clawkson_db::knowledge_base::get_by_id(pool, kb_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if is_admin || kb.owner_id == user_id {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

// ── Knowledge base CRUD ───────────────────────────────────────────

async fn list_bases(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<KnowledgeBase>>, StatusCode> {
    let pool = state.db.pool();
    let rows = if auth.is_admin() {
        clawkson_db::knowledge_base::list_all(pool).await
    } else {
        clawkson_db::knowledge_base::list_for_user(pool, auth.id()).await
    };
    let rows = rows.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.iter().map(row_to_kb).collect()))
}

/// GET /api/knowledge/agent-memories — list all agent memory KBs for the user's agents.
async fn list_agent_memories(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<KnowledgeBase>>, StatusCode> {
    let pool = state.db.pool();
    let rows = clawkson_db::knowledge_base::list_agent_memory_kbs(pool, auth.id())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let kbs = rows.iter().map(|r| KnowledgeBase {
        id: r.id,
        owner_id: r.owner_id,
        agent_id: r.agent_id,
        agent_name: Some(r.agent_name.clone()),
        name: r.name.clone(),
        description: r.description.clone(),
        kb_type: r.kb_type.clone(),
        embedding_model: r.embedding_model.clone(),
        entry_count: r.entry_count,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }).collect();
    Ok(Json(kbs))
}

async fn get_base(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<KnowledgeBase>, StatusCode> {
    let pool = state.db.pool();
    let has_access = check_access(pool, id, auth.id(), auth.is_admin()).await?;
    if !has_access {
        return Err(StatusCode::FORBIDDEN);
    }
    let row = clawkson_db::knowledge_base::get_by_id(pool, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(row_to_kb(&row)))
}

async fn create_base(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateKbRequest>,
) -> Result<Json<KnowledgeBase>, StatusCode> {
    let pool = state.db.pool();
    let model = req.embedding_model.unwrap_or_else(|| "qwen3-embedding:8b".to_string());
    let row = clawkson_db::knowledge_base::create(
        pool,
        auth.id(),
        &req.name,
        req.description.as_deref().unwrap_or(""),
        &model,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(KnowledgeBase {
        id: row.id,
        owner_id: row.owner_id,
        agent_id: row.agent_id,
        agent_name: None,
        name: row.name,
        description: row.description,
        kb_type: row.kb_type,
        embedding_model: row.embedding_model,
        entry_count: 0,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }))
}

async fn patch_base(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchKbRequest>,
) -> Result<Json<KnowledgeBase>, StatusCode> {
    let pool = state.db.pool();
    check_owner(pool, id, auth.id(), auth.is_admin()).await?;

    let existing = clawkson_db::knowledge_base::get_by_id(pool, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let name = req.name.unwrap_or(existing.name);
    let desc = req.description.unwrap_or(existing.description);

    clawkson_db::knowledge_base::update(pool, id, &name, &desc)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Re-fetch with count
    let full = clawkson_db::knowledge_base::get_by_id(pool, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(row_to_kb(&full)))
}

async fn delete_base(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    let pool = state.db.pool();
    if check_owner(pool, id, auth.id(), auth.is_admin()).await.is_err() {
        return StatusCode::FORBIDDEN;
    }

    // Prevent deletion of memory knowledge bases
    if clawkson_db::knowledge_base::is_memory_kb(pool, id).await.unwrap_or(false) {
        return StatusCode::FORBIDDEN;
    }

    // Best-effort cleanup of S3 objects before DB cascade delete
    if let Some(ref s3) = state.s3 {
        if let Ok(docs) = clawkson_db::knowledge_document::list_for_kb(pool, id).await {
            for doc in &docs {
                if let Err(e) = s3.delete_object(&doc.s3_key).await {
                    tracing::warn!(
                        kb_id = %id,
                        doc_id = %doc.id,
                        s3_key = %doc.s3_key,
                        error = %e,
                        "Failed to delete S3 object during KB deletion"
                    );
                }
            }
        }
    }

    match clawkson_db::knowledge_base::delete(pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

// ── Entries ───────────────────────────────────────────────────────

async fn list_entries(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(kb_id): Path<Uuid>,
) -> Result<Json<Vec<KnowledgeEntry>>, StatusCode> {
    let pool = state.db.pool();
    let has_access = check_access(pool, kb_id, auth.id(), auth.is_admin()).await?;
    if !has_access {
        return Err(StatusCode::FORBIDDEN);
    }
    let rows = clawkson_db::knowledge_entry::list_for_kb(pool, kb_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.iter().map(row_to_entry).collect()))
}

async fn create_entry(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(kb_id): Path<Uuid>,
    Json(req): Json<CreateEntryRequest>,
) -> Result<Json<KnowledgeEntry>, (StatusCode, Json<serde_json::Value>)> {
    let pool = state.db.pool();
    check_owner(pool, kb_id, auth.id(), auth.is_admin())
        .await
        .map_err(|s| (s, Json(serde_json::json!({"error": s.canonical_reason().unwrap_or("Forbidden")}))))?;

    if req.title.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Title cannot be empty"})),
        ));
    }
    if req.content.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Content cannot be empty"})),
        ));
    }

    tracing::info!(kb_id = %kb_id, title = %req.title, content_len = req.content.len(), "Creating knowledge entry");

    let row = clawkson_db::knowledge_entry::create(pool, kb_id, &req.title, &req.content, None)
        .await
        .map_err(|e| {
            tracing::error!(kb_id = %kb_id, title = %req.title, error = %e, "Failed to create knowledge entry");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Database error: {e}")})),
            )
        })?;

    tracing::info!(kb_id = %kb_id, entry_id = %row.id, title = %row.title, "Knowledge entry created");
    Ok(Json(row_to_entry(&row)))
}

async fn patch_entry(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((kb_id, entry_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<PatchEntryRequest>,
) -> Result<Json<KnowledgeEntry>, StatusCode> {
    let pool = state.db.pool();
    check_owner(pool, kb_id, auth.id(), auth.is_admin()).await?;

    let existing = clawkson_db::knowledge_entry::get_by_id(pool, entry_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if existing.knowledge_base_id != kb_id {
        return Err(StatusCode::NOT_FOUND);
    }

    let title = req.title.unwrap_or(existing.title);
    let content = req.content.unwrap_or(existing.content);

    let row = clawkson_db::knowledge_entry::update(pool, entry_id, &title, &content)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(row_to_entry(&row)))
}

async fn delete_entry(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((kb_id, entry_id)): Path<(Uuid, Uuid)>,
) -> StatusCode {
    let pool = state.db.pool();
    if check_owner(pool, kb_id, auth.id(), auth.is_admin()).await.is_err() {
        return StatusCode::FORBIDDEN;
    }
    match clawkson_db::knowledge_entry::delete(pool, entry_id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

// ── File upload with text extraction & chunking ──────────────────

/// Max characters per chunk (~2000 tokens). We split on paragraph boundaries.
const CHUNK_MAX_CHARS: usize = 4000;
/// Overlap between chunks to preserve context across boundaries.
const CHUNK_OVERLAP_CHARS: usize = 200;

/// Convert a DB connector row to the core `LlmConnector` type.
fn db_connector_row_to_core(row: clawkson_db::llm_connector::LlmConnectorRow) -> clawkson_core::LlmConnector {
    use clawkson_core::LlmProviderType;
    clawkson_core::LlmConnector {
        id: row.id,
        name: row.name,
        provider_type: match row.provider_type {
            clawkson_db::llm_connector::LlmProviderType::Azure => LlmProviderType::Azure,
            clawkson_db::llm_connector::LlmProviderType::Openrouter => LlmProviderType::OpenRouter,
            clawkson_db::llm_connector::LlmProviderType::Openai => LlmProviderType::OpenAi,
            clawkson_db::llm_connector::LlmProviderType::Custom => LlmProviderType::Custom,
        },
        api_key: row.api_key,
        api_base_url: row.api_base_url,
        model: row.model,
        azure_deployment: row.azure_deployment,
        azure_api_version: row.azure_api_version,
        shared_with_all: row.shared_with_all,
        created_at: row.created_at,
    }
}

/// Extract text from a file based on its extension.
fn extract_text(filename: &str, bytes: &[u8]) -> Result<String, String> {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "pdf" => {
            pdf_extract::extract_text_from_mem(bytes)
                .map_err(|e| format!("PDF extraction failed: {e}"))
        }
        "txt" | "md" | "csv" | "json" => {
            String::from_utf8(bytes.to_vec())
                .map_err(|e| format!("Invalid UTF-8: {e}"))
        }
        _ => Err(format!("Unsupported file type: .{ext}")),
    }
}

/// Split text into chunks on paragraph boundaries with overlap.
fn chunk_text(text: &str) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return vec![];
    }
    if text.len() <= CHUNK_MAX_CHARS {
        return vec![text.to_string()];
    }

    // Split into paragraphs (double newline or single newline for dense text)
    let paragraphs: Vec<&str> = text.split("\n\n").collect();

    let mut chunks = Vec::new();
    let mut current = String::new();

    for para in &paragraphs {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }

        // If adding this paragraph would exceed the limit, finalize current chunk
        if !current.is_empty() && current.len() + para.len() + 2 > CHUNK_MAX_CHARS {
            chunks.push(current.clone());

            // Start new chunk with overlap from end of previous
            let overlap_start = current.len().saturating_sub(CHUNK_OVERLAP_CHARS);
            // Find a word boundary for the overlap
            let overlap_start = current[overlap_start..]
                .find(char::is_whitespace)
                .map(|i| overlap_start + i + 1)
                .unwrap_or(overlap_start);
            current = current[overlap_start..].to_string();
        }

        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(para);
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    // If paragraphs were too large individually, do a hard split on sentence/word boundaries
    let mut final_chunks = Vec::new();
    for chunk in chunks {
        if chunk.len() <= CHUNK_MAX_CHARS {
            final_chunks.push(chunk);
        } else {
            // Hard split on sentence boundaries
            let mut remaining = chunk.as_str();
            while remaining.len() > CHUNK_MAX_CHARS {
                let split_at = remaining[..CHUNK_MAX_CHARS]
                    .rfind(". ")
                    .or_else(|| remaining[..CHUNK_MAX_CHARS].rfind('\n'))
                    .or_else(|| remaining[..CHUNK_MAX_CHARS].rfind(' '))
                    .unwrap_or(CHUNK_MAX_CHARS);
                let split_at = split_at + 1; // include the delimiter
                final_chunks.push(remaining[..split_at].trim().to_string());
                let overlap_start = split_at.saturating_sub(CHUNK_OVERLAP_CHARS);
                remaining = &remaining[overlap_start..];
            }
            if !remaining.trim().is_empty() {
                final_chunks.push(remaining.trim().to_string());
            }
        }
    }

    final_chunks
}

/// Use an LLM to find a semantically appropriate sentence boundary within the
/// `window` string (which is at most `CHUNK_MAX_CHARS` bytes long).  Returns
/// the byte offset at which the chunk should be cut, or `None` if the LLM
/// response cannot be parsed / the call fails (callers fall back to heuristics).
///
/// We only pass a small context window to the LLM — not the full document —
/// so this is cheap and fast.
async fn llm_find_split(connector: &clawkson_core::LlmConnector, window: &str) -> Option<usize> {
    let system = "You are a document chunking assistant. \
        Given a text excerpt, find the best position to split it into two semantically coherent chunks. \
        Respond with ONLY a single integer: the character offset (0-indexed) in the given text \
        at which the split should occur. \
        Choose a position that ends a complete sentence or paragraph. \
        Do not include any explanation or additional text.";

    let user_msg = format!(
        "Find the best split position in the following text (respond with a single integer offset):\n\n{}",
        window
    );

    let history = vec![(MessageRole::User, user_msg, vec![])];
    match crate::llm::complete(connector, Some(system), &history, Some(0.0), Some(16), None, 60).await {
        Ok(cr) => {
            let trimmed = cr.text.trim();
            trimmed.parse::<usize>().ok().filter(|&pos| pos > 0 && pos < window.len())
        }
        Err(e) => {
            tracing::warn!(error = %e, "ETL LLM split failed, falling back to heuristic");
            None
        }
    }
}

/// Chunk text using an LLM to find optimal sentence boundaries when paragraphs
/// are too large for the heuristic splitter alone.
async fn chunk_text_semantic(text: &str, connector: &clawkson_core::LlmConnector) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return vec![];
    }
    if text.len() <= CHUNK_MAX_CHARS {
        return vec![text.to_string()];
    }

    // Split on paragraph boundaries first (same as heuristic approach).
    let paragraphs: Vec<&str> = text.split("\n\n").collect();
    let mut chunks = Vec::new();
    let mut current = String::new();

    for para in &paragraphs {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }

        if !current.is_empty() && current.len() + para.len() + 2 > CHUNK_MAX_CHARS {
            chunks.push(current.clone());
            let overlap_start = current.len().saturating_sub(CHUNK_OVERLAP_CHARS);
            let overlap_start = current[overlap_start..]
                .find(char::is_whitespace)
                .map(|i| overlap_start + i + 1)
                .unwrap_or(overlap_start);
            current = current[overlap_start..].to_string();
        }

        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(para);
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    // For oversized chunks, ask the LLM for the best split point, falling back
    // to sentence/word heuristics if the LLM call fails.
    let mut final_chunks = Vec::new();
    for chunk in chunks {
        if chunk.len() <= CHUNK_MAX_CHARS {
            final_chunks.push(chunk);
        } else {
            let mut remaining = chunk.as_str();
            while remaining.len() > CHUNK_MAX_CHARS {
                // Take a window up to the limit and let the LLM find the best cut.
                let window = &remaining[..CHUNK_MAX_CHARS];
                let split_at = if let Some(llm_pos) = llm_find_split(connector, window).await {
                    llm_pos
                } else {
                    // Heuristic fallback
                    window
                        .rfind(". ")
                        .or_else(|| window.rfind('\n'))
                        .or_else(|| window.rfind(' '))
                        .map(|p| p + 1)
                        .unwrap_or(CHUNK_MAX_CHARS)
                };
                final_chunks.push(remaining[..split_at].trim().to_string());
                let overlap_start = split_at.saturating_sub(CHUNK_OVERLAP_CHARS);
                remaining = &remaining[overlap_start..];
            }
            if !remaining.trim().is_empty() {
                final_chunks.push(remaining.trim().to_string());
            }
        }
    }

    final_chunks
}

async fn upload_files(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(kb_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<UploadResult>, (StatusCode, Json<serde_json::Value>)> {
    let pool = state.db.pool();
    check_owner(pool, kb_id, auth.id(), auth.is_admin())
        .await
        .map_err(|s| (s, Json(serde_json::json!({"error": "Access denied"}))))?;

    let mut files_processed = 0usize;
    let mut entries_created = 0usize;
    let mut errors = Vec::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let filename = field
            .file_name()
            .unwrap_or("unnamed")
            .to_string();

        tracing::info!(
            kb_id = %kb_id,
            filename = %filename,
            "Processing uploaded file"
        );

        let bytes = match field.bytes().await {
            Ok(b) => b,
            Err(e) => {
                let msg = format!("{filename}: failed to read file data: {e}");
                tracing::error!(msg);
                errors.push(msg);
                continue;
            }
        };

        tracing::info!(
            kb_id = %kb_id,
            filename = %filename,
            size_bytes = bytes.len(),
            "File data received, extracting text"
        );

        // ── Store original document in S3 ─────────────────────────
        let doc_id = Uuid::new_v4();
        let content_type = guess_content_type(&filename);
        let s3_key = format!("{kb_id}/{doc_id}/{filename}");
        let size_bytes = bytes.len() as i64;

        let mut source_document_id: Option<Uuid> = None;

        if let Some(ref s3) = state.s3 {
            match s3.put_object(&s3_key, bytes.to_vec(), content_type).await {
                Ok(()) => {
                    match clawkson_db::knowledge_document::create(
                        pool, doc_id, kb_id, &filename, content_type, &s3_key, size_bytes,
                    )
                    .await
                    {
                        Ok(_doc_row) => {
                            tracing::info!(
                                kb_id = %kb_id,
                                doc_id = %doc_id,
                                filename = %filename,
                                "Document stored in S3"
                            );
                            source_document_id = Some(doc_id);
                        }
                        Err(e) => {
                            tracing::error!(
                                kb_id = %kb_id,
                                doc_id = %doc_id,
                                error = %e,
                                "Failed to create document record, S3 object orphaned"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        kb_id = %kb_id,
                        filename = %filename,
                        error = %e,
                        "S3 upload failed, proceeding without document storage"
                    );
                }
            }
        }

        // ── Extract text and chunk ────────────────────────────────
        let text = match extract_text(&filename, &bytes) {
            Ok(t) => t,
            Err(e) => {
                let msg = format!("{filename}: {e}");
                tracing::error!(kb_id = %kb_id, error = %msg, "Text extraction failed");
                errors.push(msg);
                continue;
            }
        };

        let text = text.replace('\0', ""); // strip null bytes
        if text.trim().is_empty() {
            let msg = format!("{filename}: no text content extracted");
            tracing::warn!(kb_id = %kb_id, msg);
            errors.push(msg);
            continue;
        }

        let base_title = filename
            .rsplit('/')
            .next()
            .unwrap_or(&filename)
            .rsplit_once('.')
            .map(|(name, _)| name)
            .unwrap_or(&filename);

        // ── Semantic or heuristic chunking ───────────────────────
        // Resolve the ETL LLM connector from app settings (best-effort; fall back if unavailable).
        let chunks = {
            let etl_connector: Option<clawkson_core::LlmConnector> = async {
                let settings = clawkson_db::settings::get(&state.db).await.ok()?;
                let etl_id = settings.etl_llm_connector_id?;
                let row = clawkson_db::llm_connector::get_by_id(&state.db, etl_id).await.ok()??;
                Some(db_connector_row_to_core(row))
            }.await;

            if let Some(ref connector) = etl_connector {
                tracing::info!(
                    kb_id = %kb_id,
                    filename = %filename,
                    connector_id = %connector.id,
                    connector_name = %connector.name,
                    "Using LLM semantic chunking for ETL"
                );
                chunk_text_semantic(&text, connector).await
            } else {
                chunk_text(&text)
            }
        };
        let total_chunks = chunks.len();

        tracing::info!(
            kb_id = %kb_id,
            filename = %filename,
            text_len = text.len(),
            chunks = total_chunks,
            "Text extracted, creating entries"
        );

        files_processed += 1;

        for (i, chunk) in chunks.iter().enumerate() {
            let title = if total_chunks == 1 {
                base_title.to_string()
            } else {
                format!("{base_title} ({}/{})", i + 1, total_chunks)
            };

            match clawkson_db::knowledge_entry::create(pool, kb_id, &title, chunk, source_document_id).await {
                Ok(row) => {
                    tracing::info!(
                        kb_id = %kb_id,
                        entry_id = %row.id,
                        chunk = i + 1,
                        chunk_total = total_chunks,
                        chunk_len = chunk.len(),
                        "Chunk entry created"
                    );
                    entries_created += 1;
                }
                Err(e) => {
                    let msg = format!("{filename} chunk {}/{total_chunks}: {e}", i + 1);
                    tracing::error!(kb_id = %kb_id, error = %msg, "Failed to create chunk entry");
                    errors.push(msg);
                }
            }
        }
    }

    // ── Embed newly created entries ────────────────────────────
    let mut embedded = 0usize;
    let mut embed_failed = 0usize;

    if entries_created > 0 {
        let embed_config = load_embedding_config(&state).await;
        let kb = clawkson_db::knowledge_base::get_by_id(pool, kb_id)
            .await
            .ok()
            .flatten();

        let model = kb
            .as_ref()
            .map(|k| k.embedding_model.as_str())
            .unwrap_or(&embed_config.model);

        let unembedded = clawkson_db::knowledge_entry::list_without_embedding(pool, kb_id)
            .await
            .unwrap_or_default();

        let batch_count = (unembedded.len() + 7) / 8;
        tracing::info!(
            kb_id = %kb_id,
            model = model,
            entries = unembedded.len(),
            batches = batch_count,
            "Starting post-upload embedding generation"
        );

        for (batch_idx, chunk) in unembedded.chunks(8).enumerate() {
            let texts: Vec<String> = chunk
                .iter()
                .map(|e| format!("{}\n\n{}", e.title, e.content))
                .collect();

            tracing::info!(
                kb_id = %kb_id,
                batch = batch_idx + 1,
                batch_total = batch_count,
                batch_size = chunk.len(),
                "Processing embedding batch"
            );

            match crate::embeddings::generate(&embed_config, model, texts).await {
                Ok(vectors) => {
                    for (entry, vec) in chunk.iter().zip(vectors.iter()) {
                        if let Err(e) = clawkson_db::knowledge_entry::set_embedding(
                            pool, entry.id, vec, None,
                        ).await {
                            tracing::error!(entry_id = %entry.id, error = %e, "Failed to store embedding");
                            embed_failed += 1;
                        } else {
                            embedded += 1;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        kb_id = %kb_id,
                        batch = batch_idx + 1,
                        error = %e,
                        "Embedding generation failed for batch"
                    );
                    embed_failed += chunk.len();
                }
            }
        }

        tracing::info!(
            kb_id = %kb_id,
            embedded = embedded,
            failed = embed_failed,
            "Post-upload embedding complete"
        );
    }

    tracing::info!(
        kb_id = %kb_id,
        files_processed = files_processed,
        entries_created = entries_created,
        embedded = embedded,
        embed_failed = embed_failed,
        errors = errors.len(),
        "File upload complete"
    );

    Ok(Json(UploadResult {
        files_processed,
        entries_created,
        embedded,
        embed_failed,
        errors,
    }))
}

// ── Embedding generation ──────────────────────────────────────────

async fn embed_entries(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(kb_id): Path<Uuid>,
) -> Result<Json<EmbedResult>, StatusCode> {
    let pool = state.db.pool();
    check_owner(pool, kb_id, auth.id(), auth.is_admin()).await?;

    let kb = clawkson_db::knowledge_base::get_by_id(pool, kb_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let embed_config = load_embedding_config(&state).await;

    let entries = clawkson_db::knowledge_entry::list_without_embedding(pool, kb_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let total = entries.len();
    let batch_count = (total + 7) / 8;
    tracing::info!(
        kb_id = %kb_id,
        kb_name = %kb.name,
        model = %kb.embedding_model,
        entries = total,
        batches = batch_count,
        "Starting embedding generation"
    );

    let mut embedded = 0usize;
    let mut failed = 0usize;

    for (batch_idx, chunk) in entries.chunks(8).enumerate() {
        tracing::info!(
            kb_id = %kb_id,
            batch = batch_idx + 1,
            batch_total = batch_count,
            batch_size = chunk.len(),
            "Processing embedding batch"
        );

        let texts: Vec<String> = chunk
            .iter()
            .map(|e| format!("{}\n\n{}", e.title, e.content))
            .collect();

        match crate::embeddings::generate(&embed_config, &kb.embedding_model, texts).await {
            Ok(vectors) => {
                tracing::info!(
                    kb_id = %kb_id,
                    batch = batch_idx + 1,
                    vectors = vectors.len(),
                    dim = vectors.first().map(|v| v.len()).unwrap_or(0),
                    "Embedding response received"
                );
                for (entry, vec) in chunk.iter().zip(vectors.iter()) {
                    if let Err(e) = clawkson_db::knowledge_entry::set_embedding(
                        pool,
                        entry.id,
                        vec,
                        None,
                    )
                    .await
                    {
                        tracing::error!(entry_id = %entry.id, error = %e, "Failed to store embedding");
                        failed += 1;
                    } else {
                        embedded += 1;
                    }
                }
            }
            Err(e) => {
                tracing::error!(
                    kb_id = %kb_id,
                    batch = batch_idx + 1,
                    batch_size = chunk.len(),
                    error = %e,
                    "Embedding generation failed for batch"
                );
                failed += chunk.len();
            }
        }
    }

    tracing::info!(
        kb_id = %kb_id,
        kb_name = %kb.name,
        embedded = embedded,
        failed = failed,
        "Embedding generation complete"
    );

    Ok(Json(EmbedResult { embedded, failed }))
}

// ── Search ────────────────────────────────────────────────────────

async fn search_entries(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(kb_id): Path<Uuid>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<Vec<KnowledgeSearchResult>>, StatusCode> {
    let pool = state.db.pool();
    let has_access = check_access(pool, kb_id, auth.id(), auth.is_admin()).await?;
    if !has_access {
        return Err(StatusCode::FORBIDDEN);
    }

    let kb = clawkson_db::knowledge_base::get_by_id(pool, kb_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let embed_config = load_embedding_config(&state).await;

    tracing::info!(kb_id = %kb_id, query = %req.query, model = %kb.embedding_model, "Starting knowledge search");

    let query_vec = crate::embeddings::generate_one(&embed_config, &kb.embedding_model, &req.query)
        .await
        .map_err(|e| {
            tracing::error!(kb_id = %kb_id, error = %e, "Query embedding generation failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    tracing::info!(kb_id = %kb_id, dim = query_vec.len(), "Query embedding generated");

    let limit = req.limit.unwrap_or(5).min(20);
    let results = clawkson_db::knowledge_entry::search(pool, &[kb_id], &query_vec, limit)
        .await
        .map_err(|e| {
            tracing::error!(kb_id = %kb_id, error = %e, "Vector search query failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    tracing::info!(kb_id = %kb_id, results = results.len(), limit = limit, "Knowledge search complete");

    let out: Vec<KnowledgeSearchResult> = results
        .iter()
        .map(|r| {
            let document_url = r.source_document_id.map(|doc_id| {
                format!("/api/knowledge/{}/documents/{doc_id}/download", r.knowledge_base_id)
            });
            KnowledgeSearchResult {
                entry: KnowledgeEntry {
                    id: r.id,
                    knowledge_base_id: r.knowledge_base_id,
                    title: r.title.clone(),
                    content: r.content.clone(),
                    token_count: r.token_count,
                    has_embedding: true,
                    source_document_id: r.source_document_id,
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                },
                score: r.score,
                document_url,
            }
        })
        .collect();

    Ok(Json(out))
}

// ── Documents (original files) ────────────────────────────────────

async fn list_documents(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(kb_id): Path<Uuid>,
) -> Result<Json<Vec<KnowledgeDocument>>, StatusCode> {
    let pool = state.db.pool();
    let has_access = check_access(pool, kb_id, auth.id(), auth.is_admin()).await?;
    if !has_access {
        return Err(StatusCode::FORBIDDEN);
    }
    let rows = clawkson_db::knowledge_document::list_for_kb(pool, kb_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.iter().map(row_to_document).collect()))
}

async fn download_document(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((kb_id, doc_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, StatusCode> {
    let pool = state.db.pool();
    let has_access = check_access(pool, kb_id, auth.id(), auth.is_admin()).await?;
    if !has_access {
        return Err(StatusCode::FORBIDDEN);
    }

    let s3 = state.s3.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let doc = clawkson_db::knowledge_document::get_by_id(pool, doc_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if doc.knowledge_base_id != kb_id {
        return Err(StatusCode::NOT_FOUND);
    }

    let (bytes, content_type) = s3
        .get_object(&doc.s3_key)
        .await
        .map_err(|e| {
            tracing::error!(doc_id = %doc_id, s3_key = %doc.s3_key, error = %e, "S3 download failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let disposition = format!("attachment; filename=\"{}\"", doc.filename);

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_DISPOSITION, disposition)
        .body(Body::from(bytes))
        .unwrap())
}

async fn delete_document(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((kb_id, doc_id)): Path<(Uuid, Uuid)>,
) -> StatusCode {
    let pool = state.db.pool();
    if check_owner(pool, kb_id, auth.id(), auth.is_admin()).await.is_err() {
        return StatusCode::FORBIDDEN;
    }

    let doc = match clawkson_db::knowledge_document::get_by_id(pool, doc_id).await {
        Ok(Some(d)) if d.knowledge_base_id == kb_id => d,
        Ok(_) => return StatusCode::NOT_FOUND,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };

    // Delete from S3 (best-effort)
    if let Some(ref s3) = state.s3 {
        if let Err(e) = s3.delete_object(&doc.s3_key).await {
            tracing::warn!(doc_id = %doc_id, error = %e, "Failed to delete S3 object");
        }
    }

    match clawkson_db::knowledge_document::delete(pool, doc_id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

// ── Sharing ───────────────────────────────────────────────────────

async fn list_shares(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(kb_id): Path<Uuid>,
) -> Result<Json<Vec<ShareInfo>>, StatusCode> {
    let pool = state.db.pool();
    check_owner(pool, kb_id, auth.id(), auth.is_admin()).await?;

    let shares = clawkson_db::knowledge_base::share_list(pool, kb_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut result = Vec::new();
    for share in shares {
        if let Ok(Some(user)) = clawkson_db::user::get_by_id(pool, share.shared_with).await {
            result.push(ShareInfo {
                id: share.id,
                user_id: user.id,
                email: user.email,
                display_name: user.display_name,
                permission: match share.permission {
                    clawkson_db::share::SharePermission::Read => SharePermission::Read,
                    clawkson_db::share::SharePermission::Write => SharePermission::Write,
                },
            });
        }
    }
    Ok(Json(result))
}

async fn create_share(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(kb_id): Path<Uuid>,
    Json(req): Json<ShareRequest>,
) -> Result<Json<ShareInfo>, StatusCode> {
    let pool = state.db.pool();
    check_owner(pool, kb_id, auth.id(), auth.is_admin()).await?;

    let target = clawkson_db::user::get_by_email(pool, &req.email)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if target.id == auth.id() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let db_perm = match req.permission {
        SharePermission::Read => clawkson_db::share::SharePermission::Read,
        SharePermission::Write => clawkson_db::share::SharePermission::Write,
    };

    let _share = clawkson_db::knowledge_base::share_create(pool, kb_id, auth.id(), target.id, db_perm)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ShareInfo {
        id: _share.id,
        user_id: target.id,
        email: target.email,
        display_name: target.display_name,
        permission: req.permission,
    }))
}

async fn remove_share(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((kb_id, user_id)): Path<(Uuid, Uuid)>,
) -> StatusCode {
    let pool = state.db.pool();
    if check_owner(pool, kb_id, auth.id(), auth.is_admin()).await.is_err() {
        return StatusCode::FORBIDDEN;
    }
    match clawkson_db::knowledge_base::share_delete(pool, kb_id, user_id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

// ── Agent access ──────────────────────────────────────────────────

async fn list_agents(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(kb_id): Path<Uuid>,
) -> Result<Json<Vec<Uuid>>, StatusCode> {
    let pool = state.db.pool();
    let has_access = check_access(pool, kb_id, auth.id(), auth.is_admin()).await?;
    if !has_access {
        return Err(StatusCode::FORBIDDEN);
    }
    let ids = clawkson_db::knowledge_base::kb_list_agents(pool, kb_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ids))
}

async fn link_agent(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(kb_id): Path<Uuid>,
    Json(req): Json<LinkAgentRequest>,
) -> StatusCode {
    let pool = state.db.pool();
    if check_owner(pool, kb_id, auth.id(), auth.is_admin()).await.is_err() {
        return StatusCode::FORBIDDEN;
    }
    match clawkson_db::knowledge_base::agent_link(pool, req.agent_id, kb_id).await {
        Ok(()) => StatusCode::CREATED,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn unlink_agent(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((kb_id, agent_id)): Path<(Uuid, Uuid)>,
) -> StatusCode {
    let pool = state.db.pool();
    if check_owner(pool, kb_id, auth.id(), auth.is_admin()).await.is_err() {
        return StatusCode::FORBIDDEN;
    }
    match clawkson_db::knowledge_base::agent_unlink(pool, agent_id, kb_id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
