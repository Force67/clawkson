/// Canvas / A2UI plugin for Clawkson.
/// Provides tools for creating and manipulating visual canvas elements,
/// plus HTTP routes for element CRUD operations.
use std::collections::HashSet;

use axum::{
    extract::Path,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use clawkson_plugin::{
    ClawksonPlugin, FrontendManifest, PluginCapability, PluginContext, PluginManifest,
    PluginRoute, RouteProvider, SidebarItem, ToolContext, ToolProvider,
};
use denkwerk::DynKernelFunction;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Types ───────────────────────────────────────────────────────

/// A visual element on the canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasElement {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub element_type: String,
    pub content: serde_json::Value,
    pub position: serde_json::Value,
    pub size: serde_json::Value,
    pub style: serde_json::Value,
    pub created_at: String,
}

/// Request to create a new canvas element.
#[derive(Debug, Deserialize)]
pub struct CreateElementRequest {
    pub conversation_id: Uuid,
    pub element_type: String,
    #[serde(default)]
    pub content: serde_json::Value,
    #[serde(default)]
    pub position: serde_json::Value,
    #[serde(default)]
    pub size: serde_json::Value,
    #[serde(default)]
    pub style: serde_json::Value,
}

/// Request to update a canvas element.
#[derive(Debug, Deserialize)]
pub struct UpdateElementRequest {
    pub content: Option<serde_json::Value>,
    pub position: Option<serde_json::Value>,
    pub size: Option<serde_json::Value>,
    pub style: Option<serde_json::Value>,
}

// ── Plugin ──────────────────────────────────────────────────────

#[derive(Debug)]
pub struct CanvasPlugin {
    manifest: PluginManifest,
}

impl CanvasPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Tools);
        caps.insert(PluginCapability::Routes);

        Self {
            manifest: PluginManifest {
                name: "canvas".to_string(),
                display_name: "Canvas".to_string(),
                description: "Visual canvas for creating and arranging UI elements, diagrams, and layouts.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: Some(FrontendManifest {
                    sidebar_items: vec![SidebarItem {
                        label: "Canvas".to_string(),
                        path: "/canvas".to_string(),
                        icon: "palette".to_string(),
                        group: "resources".to_string(),
                    }],
                    routes: vec![PluginRoute {
                        path: "/canvas".to_string(),
                        component: "CanvasPage".to_string(),
                    }],
                    settings_panels: vec![],
                    connector_cards: vec![],
                    bundle_url: None,
                }),
            },
        }
    }
}

impl Default for CanvasPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for CanvasPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("Canvas plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        tracing::info!("Canvas plugin shut down");
        Ok(())
    }

    fn migrations(&self) -> Vec<&str> {
        vec![
            "CREATE TABLE IF NOT EXISTS canvas_elements (
                id UUID PRIMARY KEY,
                conversation_id UUID NOT NULL,
                element_type TEXT NOT NULL,
                content JSONB NOT NULL DEFAULT '{}',
                position JSONB NOT NULL DEFAULT '{\"x\": 0, \"y\": 0}',
                size JSONB NOT NULL DEFAULT '{\"width\": 200, \"height\": 100}',
                style JSONB NOT NULL DEFAULT '{}',
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )",
        ]
    }
}

#[async_trait::async_trait]
impl ToolProvider for CanvasPlugin {
    async fn tools(&self, _ctx: &ToolContext) -> Vec<DynKernelFunction> {
        tracing::info!("Canvas: listing tools (stub)");

        // In a full implementation, this would return three DynKernelFunctions:
        //
        // 1. canvas_create_element(conversation_id: String, element_type: String,
        //    content?: Object, position?: Object, size?: Object, style?: Object) -> Object
        //    Creates a new visual element on the canvas.
        //
        // 2. canvas_update_element(element_id: String, content?: Object,
        //    position?: Object, size?: Object, style?: Object) -> Object
        //    Updates properties of an existing canvas element.
        //
        // 3. canvas_layout(conversation_id: String, layout: String,
        //    element_ids?: [String]) -> Object
        //    Applies an automatic layout algorithm (grid, tree, force-directed)
        //    to the specified elements or all elements on the canvas.
        Vec::new()
    }
}

impl RouteProvider for CanvasPlugin {
    fn prefix(&self) -> &str {
        "/api/plugins/canvas"
    }

    fn routes(&self) -> Router {
        Router::new()
            .route("/elements", get(list_elements).post(create_element))
            .route(
                "/elements/{id}",
                axum::routing::patch(update_element).delete(delete_element),
            )
    }
}

// ── Route Handlers ──────────────────────────────────────────────

/// GET /api/plugins/canvas/elements
/// Lists all canvas elements, optionally filtered by conversation_id query param.
async fn list_elements() -> impl IntoResponse {
    tracing::info!("Canvas: list elements (stub)");
    Json(serde_json::json!({
        "elements": [],
        "total": 0
    }))
}

/// POST /api/plugins/canvas/elements
/// Creates a new canvas element.
async fn create_element(
    Json(payload): Json<CreateElementRequest>,
) -> impl IntoResponse {
    tracing::info!(
        conversation_id = %payload.conversation_id,
        element_type = %payload.element_type,
        "Canvas: create element (stub)"
    );
    let element = CanvasElement {
        id: Uuid::new_v4(),
        conversation_id: payload.conversation_id,
        element_type: payload.element_type,
        content: payload.content,
        position: payload.position,
        size: payload.size,
        style: payload.style,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    (axum::http::StatusCode::CREATED, Json(serde_json::json!(element)))
}

/// PATCH /api/plugins/canvas/elements/{id}
/// Updates an existing canvas element.
async fn update_element(
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateElementRequest>,
) -> impl IntoResponse {
    tracing::info!(
        element_id = %id,
        has_content = payload.content.is_some(),
        has_position = payload.position.is_some(),
        has_size = payload.size.is_some(),
        has_style = payload.style.is_some(),
        "Canvas: update element (stub)"
    );
    Json(serde_json::json!({
        "id": id,
        "updated": true
    }))
}

/// DELETE /api/plugins/canvas/elements/{id}
/// Deletes a canvas element.
async fn delete_element(
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    tracing::info!(element_id = %id, "Canvas: delete element (stub)");
    axum::http::StatusCode::NO_CONTENT
}
