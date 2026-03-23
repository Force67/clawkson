use axum::{
    extract::State,
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
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
        }));
    }

    let limit = req.limit.clamp(1, 20);
    let user_id = auth.id();
    let is_admin = auth.is_admin();

    // Run all searches in parallel
    let (msg_hits, conv_title_hits, agent_rows, kb_rows) = tokio::join!(
        clawkson_db::message::search_content(&state.db, user_id, is_admin, query, limit),
        clawkson_db::conversation::search_by_title(&state.db, user_id, is_admin, query, limit),
        clawkson_db::agent::list_for_user(&state.db, user_id, is_admin),
        clawkson_db::knowledge_base::search_by_text(state.db.pool(), user_id, is_admin, query, limit),
    );

    // Merge conversation results: title matches + message content matches
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

    // Filter agents by query
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

    let knowledge_bases: Vec<KnowledgeBaseHit> = kb_rows
        .unwrap_or_default()
        .into_iter()
        .map(|kb| KnowledgeBaseHit {
            id: kb.id,
            name: kb.name,
            description: kb.description,
            entry_count: kb.entry_count,
        })
        .collect();

    Ok(Json(SearchResponse {
        conversations,
        agents,
        knowledge_bases,
    }))
}
