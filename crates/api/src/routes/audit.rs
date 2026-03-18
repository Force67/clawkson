use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_all))
        .route("/stats", get(user_stats))
        .route("/conversations/{conv_id}", get(list_by_conversation))
        .route("/agents/{agent_id}", get(list_by_agent))
        .route("/denied", get(list_denied))
        .route("/conversations/{conv_id}/stats", get(conversation_stats))
}

#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

/// Filters for the global audit log list.
#[derive(Debug, Deserialize)]
pub struct AuditListParams {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub agent_id: Option<Uuid>,
    pub tool_name: Option<String>,
    pub decision: Option<String>,
    /// ISO 8601 timestamp — only return entries created after this.
    pub since: Option<DateTime<Utc>>,
}

/// Global audit log list for the current user, with optional filters.
async fn list_all(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<AuditListParams>,
) -> Result<Json<Vec<clawkson_db::tool_audit::ToolAuditEnrichedRow>>, StatusCode> {
    let rows = clawkson_db::tool_audit::list_for_user(
        &state.db,
        auth.id(),
        params.agent_id,
        params.tool_name.as_deref(),
        params.decision.as_deref(),
        params.since,
        params.limit.min(200),
        params.offset,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}

/// Aggregate stats for the current user.
#[derive(Debug, Deserialize)]
pub struct StatsParams {
    pub since: Option<DateTime<Utc>>,
}

async fn user_stats(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<StatsParams>,
) -> Result<Json<clawkson_db::tool_audit::UserAuditStats>, StatusCode> {
    let stats = clawkson_db::tool_audit::stats_for_user(
        &state.db,
        auth.id(),
        params.since,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(stats))
}

/// List audit entries for a conversation.
async fn list_by_conversation(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(conv_id): Path<Uuid>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Vec<clawkson_db::tool_audit::ToolAuditRow>>, StatusCode> {
    // Verify the user owns the conversation (or is admin)
    let conv = clawkson_db::conversation::get_by_id(&state.db, conv_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if conv.owner_id != Some(auth.id()) && !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let rows = clawkson_db::tool_audit::list_by_conversation(
        &state.db,
        conv_id,
        params.limit.min(200),
        params.offset,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}

/// List audit entries for an agent.
async fn list_by_agent(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Vec<clawkson_db::tool_audit::ToolAuditRow>>, StatusCode> {
    // Verify the user owns the agent (or is admin)
    let agent = clawkson_db::agent::get_by_id(&state.db, agent_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if agent.owner_id != Some(auth.id()) && !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let rows = clawkson_db::tool_audit::list_by_agent(
        &state.db,
        agent_id,
        params.limit.min(200),
        params.offset,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}

/// List denied audit entries for the current user.
async fn list_denied(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Vec<clawkson_db::tool_audit::ToolAuditRow>>, StatusCode> {
    let rows = clawkson_db::tool_audit::list_denied_for_user(
        &state.db,
        auth.id(),
        params.limit.min(200),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}

/// Get audit stats (allowed/denied counts) for a conversation.
async fn conversation_stats(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(conv_id): Path<Uuid>,
) -> Result<Json<Vec<clawkson_db::tool_audit::AuditStats>>, StatusCode> {
    let conv = clawkson_db::conversation::get_by_id(&state.db, conv_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if conv.owner_id != Some(auth.id()) && !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let stats = clawkson_db::tool_audit::stats_by_conversation(&state.db, conv_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(stats))
}

// ── Policy Presets ──────────────────────────────────────────────────

pub fn presets_router() -> Router<AppState> {
    Router::new().route("/", get(list_presets))
}

/// Built-in policy presets for common connector use-cases.
/// These are static and compiled into the binary — no DB needed.
async fn list_presets(
    _auth: AuthUser,
) -> Json<Vec<PresetResponse>> {
    Json(built_in_presets())
}

#[derive(Debug, Clone, Serialize)]
pub struct PresetResponse {
    pub name: String,
    pub label: String,
    pub connector_type: String,
    pub policy: PresetPolicyResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct PresetPolicyResponse {
    pub allow: Vec<PresetRuleResponse>,
    pub deny: Vec<PresetRuleResponse>,
    pub rate_limit_rpm: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PresetRuleResponse {
    pub methods: Vec<String>,
    pub path_pattern: String,
    pub description: String,
}

fn built_in_presets() -> Vec<PresetResponse> {
    vec![
        PresetResponse {
            name: "gmail_read_only".to_string(),
            label: "Gmail — Read Only".to_string(),
            connector_type: "gmail".to_string(),
            policy: PresetPolicyResponse {
                allow: vec![PresetRuleResponse {
                    methods: vec!["GET".to_string()],
                    path_pattern: "/gmail/v1/users/me/**".to_string(),
                    description: "Read messages, labels, threads".to_string(),
                }],
                deny: vec![],
                rate_limit_rpm: Some(60),
            },
        },
        PresetResponse {
            name: "gmail_read_send".to_string(),
            label: "Gmail — Read & Send".to_string(),
            connector_type: "gmail".to_string(),
            policy: PresetPolicyResponse {
                allow: vec![
                    PresetRuleResponse {
                        methods: vec!["GET".to_string()],
                        path_pattern: "/gmail/v1/users/me/**".to_string(),
                        description: "Read messages, labels, threads".to_string(),
                    },
                    PresetRuleResponse {
                        methods: vec!["POST".to_string()],
                        path_pattern: "/gmail/v1/users/me/messages/send".to_string(),
                        description: "Send new messages".to_string(),
                    },
                ],
                deny: vec![
                    PresetRuleResponse {
                        methods: vec!["DELETE".to_string()],
                        path_pattern: "/gmail/v1/**".to_string(),
                        description: "Block all delete operations".to_string(),
                    },
                ],
                rate_limit_rpm: Some(30),
            },
        },
        PresetResponse {
            name: "azure_devops_read_only".to_string(),
            label: "Azure DevOps — Read Only".to_string(),
            connector_type: "azure_devops".to_string(),
            policy: PresetPolicyResponse {
                allow: vec![PresetRuleResponse {
                    methods: vec!["GET".to_string()],
                    path_pattern: "/**".to_string(),
                    description: "Read all Azure DevOps resources".to_string(),
                }],
                deny: vec![],
                rate_limit_rpm: Some(120),
            },
        },
        PresetResponse {
            name: "azure_devops_work_items".to_string(),
            label: "Azure DevOps — Read & Manage Work Items".to_string(),
            connector_type: "azure_devops".to_string(),
            policy: PresetPolicyResponse {
                allow: vec![
                    PresetRuleResponse {
                        methods: vec!["GET".to_string()],
                        path_pattern: "/**".to_string(),
                        description: "Read all resources".to_string(),
                    },
                    PresetRuleResponse {
                        methods: vec!["POST".to_string(), "PATCH".to_string()],
                        path_pattern: "/*/_apis/wit/workitems/**".to_string(),
                        description: "Create and update work items".to_string(),
                    },
                ],
                deny: vec![
                    PresetRuleResponse {
                        methods: vec!["DELETE".to_string()],
                        path_pattern: "/**".to_string(),
                        description: "Block all delete operations".to_string(),
                    },
                ],
                rate_limit_rpm: Some(60),
            },
        },
        PresetResponse {
            name: "telegram_read_send".to_string(),
            label: "Telegram — Read & Send Messages".to_string(),
            connector_type: "telegram".to_string(),
            policy: PresetPolicyResponse {
                allow: vec![
                    PresetRuleResponse {
                        methods: vec!["GET".to_string(), "POST".to_string()],
                        path_pattern: "/bot*/getUpdates".to_string(),
                        description: "Read updates".to_string(),
                    },
                    PresetRuleResponse {
                        methods: vec!["POST".to_string()],
                        path_pattern: "/bot*/sendMessage".to_string(),
                        description: "Send messages".to_string(),
                    },
                ],
                deny: vec![],
                rate_limit_rpm: Some(30),
            },
        },
        PresetResponse {
            name: "slack_read_only".to_string(),
            label: "Slack — Read Only".to_string(),
            connector_type: "slack".to_string(),
            policy: PresetPolicyResponse {
                allow: vec![PresetRuleResponse {
                    methods: vec!["GET".to_string(), "POST".to_string()],
                    path_pattern: "/api/conversations.*".to_string(),
                    description: "Read conversations and channels".to_string(),
                }],
                deny: vec![
                    PresetRuleResponse {
                        methods: vec!["POST".to_string()],
                        path_pattern: "/api/chat.postMessage".to_string(),
                        description: "Block sending messages".to_string(),
                    },
                    PresetRuleResponse {
                        methods: vec!["POST".to_string()],
                        path_pattern: "/api/chat.delete".to_string(),
                        description: "Block deleting messages".to_string(),
                    },
                ],
                rate_limit_rpm: Some(60),
            },
        },
        PresetResponse {
            name: "custom_read_only".to_string(),
            label: "Custom — GET Only".to_string(),
            connector_type: "custom".to_string(),
            policy: PresetPolicyResponse {
                allow: vec![PresetRuleResponse {
                    methods: vec!["GET".to_string()],
                    path_pattern: "/**".to_string(),
                    description: "Allow all GET requests".to_string(),
                }],
                deny: vec![],
                rate_limit_rpm: None,
            },
        },
        PresetResponse {
            name: "custom_full_access".to_string(),
            label: "Custom — Full Access".to_string(),
            connector_type: "custom".to_string(),
            policy: PresetPolicyResponse {
                allow: vec![PresetRuleResponse {
                    methods: vec!["GET".to_string(), "POST".to_string(), "PUT".to_string(), "PATCH".to_string(), "DELETE".to_string()],
                    path_pattern: "/**".to_string(),
                    description: "Allow all methods and paths".to_string(),
                }],
                deny: vec![],
                rate_limit_rpm: None,
            },
        },
    ]
}
