use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{Duration, Utc};
use clawkson_core::UsageSummaryWithCost;
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/me", get(get_my_usage))
}

#[derive(Debug, Deserialize)]
pub struct UsageQuery {
    pub since: Option<String>,
}

fn parse_since(s: &str) -> Option<chrono::DateTime<Utc>> {
    let now = Utc::now();
    match s {
        "24h" => Some(now - Duration::hours(24)),
        "7d" => Some(now - Duration::days(7)),
        "30d" => Some(now - Duration::days(30)),
        _ => None,
    }
}

/// GET /api/usage/me — current user's usage with cost estimates
async fn get_my_usage(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<UsageQuery>,
) -> Result<Json<Vec<UsageSummaryWithCost>>, StatusCode> {
    let since = q.since.as_deref().and_then(parse_since);
    let rows = clawkson_db::token_usage::get_user_summary_with_cost(&state.db, auth.id(), since)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let summaries: Vec<UsageSummaryWithCost> = rows
        .into_iter()
        .map(|r| UsageSummaryWithCost {
            model: r.model,
            prompt_tokens: r.prompt_tokens,
            completion_tokens: r.completion_tokens,
            total_tokens: r.total_tokens,
            estimated_cost_usd: r.estimated_cost_usd,
        })
        .collect();

    Ok(Json(summaries))
}
