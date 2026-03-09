use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use clawkson_core::{ConversationShare, SharePermission};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/conversations/{id}/shares",
            get(list_shares).post(create_share),
        )
        .route(
            "/conversations/{conversation_id}/shares/{user_id}",
            axum::routing::delete(remove_share),
        )
}

#[derive(Debug, Deserialize)]
pub struct CreateShareRequest {
    /// Email of the user to share with.
    pub email: String,
    pub permission: SharePermission,
}

#[derive(Debug, Serialize)]
pub struct ShareResponse {
    pub share: ConversationShare,
    pub shared_with_user: ShareUserInfo,
}

#[derive(Debug, Serialize)]
pub struct ShareUserInfo {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
}

/// GET /api/conversations/{id}/shares — list who a conversation is shared with
async fn list_shares(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(conv_id): Path<Uuid>,
) -> Result<Json<Vec<ShareResponse>>, StatusCode> {
    let pool = state.db.pool();

    // Verify user owns the conversation or is admin
    let owns = conversation_owner_check(&state, conv_id, auth.id(), auth.is_admin()).await?;
    if !owns {
        return Err(StatusCode::FORBIDDEN);
    }

    let shares = clawkson_db::share::list_for_conversation(pool, conv_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut result = Vec::new();
    for share in shares {
        if let Ok(Some(user_row)) = clawkson_db::user::get_by_id(pool, share.shared_with).await {
            result.push(ShareResponse {
                share: ShareRow_to_share(&share),
                shared_with_user: ShareUserInfo {
                    id: user_row.id,
                    email: user_row.email,
                    display_name: user_row.display_name,
                },
            });
        }
    }

    Ok(Json(result))
}

/// POST /api/conversations/{id}/shares — share a conversation with another user
async fn create_share(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(conv_id): Path<Uuid>,
    Json(req): Json<CreateShareRequest>,
) -> Result<Json<ShareResponse>, StatusCode> {
    let pool = state.db.pool();

    // Verify user owns the conversation
    let owns = conversation_owner_check(&state, conv_id, auth.id(), auth.is_admin()).await?;
    if !owns {
        return Err(StatusCode::FORBIDDEN);
    }

    // Find the user to share with
    let target_user = clawkson_db::user::get_by_email(pool, &req.email)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Can't share with yourself
    if target_user.id == auth.id() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let db_permission = match req.permission {
        SharePermission::Read => clawkson_db::share::SharePermission::Read,
        SharePermission::Write => clawkson_db::share::SharePermission::Write,
    };

    let share_row = clawkson_db::share::create(pool, conv_id, auth.id(), target_user.id, db_permission)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ShareResponse {
        share: ShareRow_to_share(&share_row),
        shared_with_user: ShareUserInfo {
            id: target_user.id,
            email: target_user.email,
            display_name: target_user.display_name,
        },
    }))
}

/// DELETE /api/conversations/{conversation_id}/shares/{user_id} — remove a share
async fn remove_share(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((conv_id, user_id)): Path<(Uuid, Uuid)>,
) -> StatusCode {
    let pool = state.db.pool();

    // Verify user owns the conversation or is admin
    let owns = match conversation_owner_check(&state, conv_id, auth.id(), auth.is_admin()).await {
        Ok(v) => v,
        Err(status) => return status,
    };
    if !owns {
        return StatusCode::FORBIDDEN;
    }

    match clawkson_db::share::delete_by_conversation_and_user(pool, conv_id, user_id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// Check if a user owns a conversation (or is admin).
async fn conversation_owner_check(
    state: &AppState,
    conv_id: Uuid,
    user_id: Uuid,
    is_admin: bool,
) -> Result<bool, StatusCode> {
    let conv = clawkson_db::conversation::get_by_id(&state.db, conv_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(is_admin || conv.owner_id == Some(user_id))
}

#[allow(non_snake_case)]
fn ShareRow_to_share(row: &clawkson_db::share::ShareRow) -> ConversationShare {
    ConversationShare {
        id: row.id,
        conversation_id: row.conversation_id,
        shared_by: row.shared_by,
        shared_with: row.shared_with,
        permission: match row.permission {
            clawkson_db::share::SharePermission::Read => SharePermission::Read,
            clawkson_db::share::SharePermission::Write => SharePermission::Write,
        },
        created_at: row.created_at,
    }
}
