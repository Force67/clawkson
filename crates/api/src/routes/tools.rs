use axum::{
    extract::State,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_tools))
}

/// A tool as returned by the list endpoint.
/// Built-in tools have no connector_id; connector-derived tools carry the connector id.
#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<Uuid>,
    /// "builtin" or "connector"
    pub tool_type: String,
    pub enabled: bool,
}

/// GET /api/tools
///
/// Returns all tools available to the calling user:
///   - Built-in tools (code execution, workspace I/O, knowledge search) are always listed.
///   - Connector tools: one `authenticated_http` entry is synthesised per enabled connector.
async fn list_tools(auth: AuthUser, State(state): State<AppState>) -> Json<Vec<ToolInfo>> {
    let mut tools: Vec<ToolInfo> = vec![
        ToolInfo {
            id: "builtin:code_execution".into(),
            name: "code_execution".into(),
            description: "Execute Python or Bash code inside the agent's sandboxed container. \
                          Input files placed under /workspace/inputs/ are accessible; \
                          files written to /workspace/outputs/ are returned inline."
                .into(),
            connector_id: None,
            tool_type: "builtin".into(),
            enabled: true,
        },
        ToolInfo {
            id: "builtin:workspace_read".into(),
            name: "workspace_read".into(),
            description: "Read the contents of a file (or list a directory) in the agent's \
                          container workspace. Paths are relative to /workspace."
                .into(),
            connector_id: None,
            tool_type: "builtin".into(),
            enabled: true,
        },
        ToolInfo {
            id: "builtin:workspace_write".into(),
            name: "workspace_write".into(),
            description: "Write text content to a file in the agent's container workspace. \
                          Parent directories are created automatically."
                .into(),
            connector_id: None,
            tool_type: "builtin".into(),
            enabled: true,
        },
        ToolInfo {
            id: "builtin:workspace_list".into(),
            name: "workspace_list".into(),
            description: "List files and directories inside the agent's container workspace."
                .into(),
            connector_id: None,
            tool_type: "builtin".into(),
            enabled: true,
        },
        ToolInfo {
            id: "builtin:knowledge_list".into(),
            name: "knowledge_list".into(),
            description: "List all knowledge bases linked to this agent, including their names, \
                          descriptions, and entry counts."
                .into(),
            connector_id: None,
            tool_type: "builtin".into(),
            enabled: true,
        },
        ToolInfo {
            id: "builtin:knowledge_search".into(),
            name: "knowledge_search".into(),
            description: "Search the agent's linked knowledge bases using semantic similarity. \
                          Returns the most relevant passages for a natural-language query."
                .into(),
            connector_id: None,
            tool_type: "builtin".into(),
            enabled: true,
        },
    ];

    // Add connector-derived tools for enabled connectors owned by this user.
    if let Ok(connectors) =
        clawkson_db::connector::list_for_user(&state.db, auth.id()).await
    {
        for c in connectors.into_iter().filter(|c| c.enabled) {
            match c.connector_type {
                clawkson_db::connector::ConnectorType::Tavily
                | clawkson_db::connector::ConnectorType::Bing => {
                    let provider_label = match c.connector_type {
                        clawkson_db::connector::ConnectorType::Tavily => "Tavily",
                        clawkson_db::connector::ConnectorType::Bing => "Bing",
                        _ => "Web",
                    };
                    tools.push(ToolInfo {
                        id: format!("connector:{}", c.id),
                        name: "web_search".into(),
                        description: format!(
                            "Search the web using {} (connector: '{}').",
                            provider_label, c.name
                        ),
                        connector_id: Some(c.id),
                        tool_type: "connector".into(),
                        enabled: true,
                    });
                }
                _ => {
                    tools.push(ToolInfo {
                        id: format!("connector:{}", c.id),
                        name: format!("authenticated_http:{}", c.name),
                        description: format!(
                            "Make authenticated HTTP requests using the '{}' connector. \
                             Credentials are injected automatically.",
                            c.name
                        ),
                        connector_id: Some(c.id),
                        tool_type: "connector".into(),
                        enabled: true,
                    });
                }
            }
        }
    }

    Json(tools)
}
