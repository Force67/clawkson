use axum::{
    extract::State,
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::embeddings::EmbeddingConfig;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", post(global_search))
}

// ── Request / Response types ─────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    8
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub conversations: Vec<ConversationHit>,
    pub agents: Vec<AgentHit>,
    pub knowledge_bases: Vec<KnowledgeBaseHit>,
    pub knowledge_entries: Vec<KnowledgeEntryHit>,
}

#[derive(Debug, Serialize)]
pub struct ConversationHit {
    pub id: Uuid,
    pub title: String,
    pub agent_id: Option<Uuid>,
    pub agent_name: Option<String>,
    pub updated_at: String,
    pub message_snippet: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AgentHit {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct KnowledgeBaseHit {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub entry_count: i64,
}

#[derive(Debug, Serialize)]
pub struct KnowledgeEntryHit {
    pub id: Uuid,
    pub knowledge_base_id: Uuid,
    pub kb_name: String,
    pub title: String,
    pub content: String,
    pub score: f64,
    /// "memory" if from an agent memory KB, "standard" otherwise
    pub source: String,
}

// ── Handler ──────────────────────────────────────────────────────

async fn global_search(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, StatusCode> {
    let query = req.query.trim();
    if query.is_empty() || query.len() < 2 {
        return Ok(Json(SearchResponse {
            conversations: vec![],
            agents: vec![],
            knowledge_bases: vec![],
            knowledge_entries: vec![],
        }));
    }

    let limit = req.limit.clamp(1, 20);
    let user_id = auth.id();
    let is_admin = auth.is_admin();
    let pool = state.db.pool();

    // Load embedding config (for semantic search)
    let embed_config = match clawkson_db::settings::get(&state.db).await {
        Ok(s) => EmbeddingConfig {
            base_url: s.embedding_api_base_url,
            api_key: s.embedding_api_key,
            model: s.embedding_model,
        },
        Err(_) => EmbeddingConfig::default(),
    };

    // Gather all KB IDs the user can access (standard + memory) for semantic search
    let (user_kbs, memory_kbs) = tokio::join!(
        clawkson_db::knowledge_base::list_for_user(pool, user_id),
        clawkson_db::knowledge_base::list_agent_memory_kbs(pool, user_id),
    );

    let mut all_kb_ids: Vec<Uuid> = Vec::new();
    let mut kb_name_map = std::collections::HashMap::<Uuid, String>::new();
    let mut memory_kb_ids = std::collections::HashSet::<Uuid>::new();

    if let Ok(ref kbs) = user_kbs {
        for kb in kbs {
            all_kb_ids.push(kb.id);
            kb_name_map.insert(kb.id, kb.name.clone());
        }
    }
    if let Ok(ref kbs) = memory_kbs {
        for kb in kbs {
            all_kb_ids.push(kb.id);
            kb_name_map.insert(
                kb.id,
                format!("{} Memory", if kb.agent_name.is_empty() { "Agent" } else { &kb.agent_name }),
            );
            memory_kb_ids.insert(kb.id);
        }
    }

    // Run ILIKE searches + embedding generation in parallel
    let model = embed_config.model.clone();
    let (msg_hits, conv_title_hits, agent_rows, kb_text_hits, query_embedding) = tokio::join!(
        clawkson_db::message::search_content(&state.db, user_id, is_admin, query, limit),
        clawkson_db::conversation::search_by_title(&state.db, user_id, is_admin, query, limit),
        clawkson_db::agent::list_for_user(&state.db, user_id, is_admin),
        clawkson_db::knowledge_base::search_by_text(pool, user_id, is_admin, query, limit),
        crate::embeddings::generate_one(&embed_config, &model, query),
    );

    // Run vector search if embedding succeeded
    let semantic_hits = if let Ok(ref vec) = query_embedding {
        if !all_kb_ids.is_empty() {
            clawkson_db::knowledge_entry::search(pool, &all_kb_ids, vec, limit)
                .await
                .unwrap_or_default()
        } else {
            vec![]
        }
    } else {
        tracing::warn!("Embedding generation failed for search query, skipping semantic search");
        vec![]
    };

    // ── Merge conversation results ───────────────────────────────

    let mut conv_map = std::collections::HashMap::<Uuid, ConversationHit>::new();

    if let Ok(titles) = conv_title_hits {
        for t in titles {
            conv_map.entry(t.id).or_insert(ConversationHit {
                id: t.id,
                title: t.title,
                agent_id: t.agent_id,
                agent_name: t.agent_name,
                updated_at: t.updated_at.to_rfc3339(),
                message_snippet: None,
            });
        }
    }

    if let Ok(msgs) = msg_hits {
        for m in msgs {
            let entry = conv_map.entry(m.conversation_id).or_insert(ConversationHit {
                id: m.conversation_id,
                title: m.conversation_title.clone(),
                agent_id: m.agent_id,
                agent_name: m.agent_name.clone(),
                updated_at: m.conversation_updated_at.to_rfc3339(),
                message_snippet: None,
            });
            entry.message_snippet = Some(m.message_snippet);
        }
    }

    let mut conversations: Vec<ConversationHit> = conv_map.into_values().collect();
    conversations.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    conversations.truncate(limit as usize);

    // ── Agents ───────────────────────────────────────────────────

    let pattern_lower = query.to_lowercase();
    let agents: Vec<AgentHit> = agent_rows
        .unwrap_or_default()
        .into_iter()
        .filter(|a| {
            a.name.to_lowercase().contains(&pattern_lower)
                || a.description.to_lowercase().contains(&pattern_lower)
        })
        .take(limit as usize)
        .map(|a| AgentHit {
            id: a.id,
            name: a.name,
            description: a.description,
            status: format!("{:?}", a.status).to_lowercase(),
        })
        .collect();

    // ── Knowledge bases (text match) ─────────────────────────────

    let knowledge_bases: Vec<KnowledgeBaseHit> = kb_text_hits
        .unwrap_or_default()
        .into_iter()
        .map(|kb| KnowledgeBaseHit {
            id: kb.id,
            name: kb.name,
            description: kb.description,
            entry_count: kb.entry_count,
        })
        .collect();

    // ── Knowledge entries (semantic vector search) ───────────────

    let knowledge_entries: Vec<KnowledgeEntryHit> = semantic_hits
        .into_iter()
        .filter(|hit| hit.score > 0.3) // drop low-relevance noise
        .take(limit as usize)
        .map(|hit| {
            let is_memory = memory_kb_ids.contains(&hit.knowledge_base_id);
            KnowledgeEntryHit {
                id: hit.id,
                knowledge_base_id: hit.knowledge_base_id,
                kb_name: kb_name_map
                    .get(&hit.knowledge_base_id)
                    .cloned()
                    .unwrap_or_default(),
                title: hit.title,
                content: if hit.content.len() > 200 {
                    format!("{}...", &hit.content[..200])
                } else {
                    hit.content
                },
                score: hit.score,
                source: if is_memory { "memory".into() } else { "standard".into() },
            }
        })
        .collect();

    Ok(Json(SearchResponse {
        conversations,
        agents,
        knowledge_bases,
        knowledge_entries,
    }))
}
