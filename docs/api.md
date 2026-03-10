# API Reference

The full API specification is maintained in [`openapi.yml`](../openapi.yml).

## Base URL

```
http://localhost:47821/api
```

## Authentication

Clawkson uses **session-based authentication via HttpOnly cookies**. On successful login the server sets a `clawkson_session` cookie that is included automatically in subsequent requests. All endpoints except `/auth/register` and `/auth/login` require a valid session.

The **first user to register** is automatically assigned the `admin` role.

### Auth Endpoints
- `POST /api/auth/register` — Register a new user (`{ username, email, password }`)
- `POST /api/auth/login` — Log in (`{ email, password }`); sets `clawkson_session` cookie
- `POST /api/auth/logout` — Log out; clears session cookie
- `GET /api/auth/me` — Returns the currently authenticated user

## Endpoints Summary

### Admin _(admin role required)_
- `GET /api/admin/users` — List all users
- `PATCH /api/admin/users/{id}/role` — Change a user's role (`{ role: "admin" | "user" }`)
- `DELETE /api/admin/users/{id}` — Delete a user

### Agents
- `GET /api/agents` — List all agents
- `POST /api/agents` — Create a new agent
- `GET /api/agents/{id}` — Get agent by ID
- `PATCH /api/agents/{id}` — Update an agent (name, description, llm_connector_id, system_prompt, temperature, max_tokens, status)
- `DELETE /api/agents/{id}` — Delete an agent
- `POST /api/agents/{id}/start` — Start the agent's Docker container
- `POST /api/agents/{id}/stop` — Stop the agent's Docker container
- `GET /api/agents/{id}/logs` — Stream container logs (SSE)
- `POST /api/agents/{id}/exec` — Execute a command inside the container
- `GET /api/agents/{id}/status` — Get container status
- `POST /api/agents/{id}/remove` — Remove the container

### Conversations
- `GET /api/conversations` — List all conversations
- `POST /api/conversations` — Create a new conversation
- `GET /api/conversations/{id}` — Get conversation by ID
- `PATCH /api/conversations/{id}` — Update a conversation
- `DELETE /api/conversations/{id}` — Delete a conversation
- `GET /api/conversations/{id}/messages` — List messages
- `POST /api/conversations/{id}/messages` — Send a raw message
- `POST /api/conversations/{id}/chat` — Send a user message and get an AI response (blocking)
- `POST /api/conversations/{id}/chat/stream` — Send a user message and stream AI response via SSE
- `GET /api/conversations/{id}/shares` — List users with access to the conversation
- `POST /api/conversations/{id}/shares` — Share a conversation with another user (`{ user_email, permission }`)
- `DELETE /api/conversations/{conversation_id}/shares/{user_id}` — Remove a conversation share

### LLM Connectors
- `GET /api/llm-connectors` — List all LLM connectors (API keys masked)
- `POST /api/llm-connectors` — Create an LLM connector
- `GET /api/llm-connectors/{id}` — Get connector by ID
- `PATCH /api/llm-connectors/{id}` — Update a connector, including provider type, model, endpoint, and credentials
- `DELETE /api/llm-connectors/{id}` — Delete a connector
- `POST /api/llm-connectors/test` — Validate connector settings against the selected provider without saving them

### Settings
- `GET /api/settings` — Get application settings
- `PATCH /api/settings` — Update application settings _(admin role required)_

Patchable fields:
| Field | Type | Description |
|---|---|---|
| `default_llm_connector_id` | `uuid \| null` | Default LLM connector used by agents |
| `etl_llm_connector_id` | `uuid \| null` | LLM connector used for semantic chunking during KB ingestion; `null` uses heuristic splitting |
| `theme` | `string` | UI theme: `dark`, `light`, or `system` |

### Knowledge Base
- `GET /api/knowledge` — List knowledge bases owned by or shared with the current user
- `POST /api/knowledge` — Create a knowledge base (`{ name, description?, embedding_model? }`)
- `GET /api/knowledge/{id}` — Get knowledge base by ID
- `PATCH /api/knowledge/{id}` — Update name/description
- `DELETE /api/knowledge/{id}` — Delete a knowledge base (owner only)
- `GET /api/knowledge/{id}/entries` — List text entries in a knowledge base
- `POST /api/knowledge/{id}/entries` — Create a text entry manually (`{ title, content }`)
- `PATCH /api/knowledge/{kb_id}/entries/{entry_id}` — Update an entry
- `DELETE /api/knowledge/{kb_id}/entries/{entry_id}` — Delete an entry
- `POST /api/knowledge/{id}/upload` — Upload one or more files (multipart/form-data); auto-chunks and embeds
- `POST /api/knowledge/{id}/embed` — (Re-)generate embeddings for all entries in a base
- `POST /api/knowledge/{id}/search` — Semantic search (`{ query, limit? }`)
- `GET /api/knowledge/{kb_id}/documents` — List uploaded source documents
- `GET /api/knowledge/{kb_id}/documents/{doc_id}/download` — Download a source document from S3
- `DELETE /api/knowledge/{kb_id}/documents/{doc_id}` — Delete a source document
- `GET /api/knowledge/{id}/shares` — List knowledge base shares
- `POST /api/knowledge/{id}/shares` — Share with a user (`{ user_email, permission: "read" | "write" }`)
- `DELETE /api/knowledge/{kb_id}/shares/{user_id}` — Remove a share
- `GET /api/knowledge/{id}/agents` — List agents linked to this knowledge base
- `POST /api/knowledge/{id}/agents` — Link an agent (`{ agent_id }`)
- `DELETE /api/knowledge/{kb_id}/agents/{agent_id}` — Unlink an agent

### Uploads (generic file attachments)
- `GET /api/uploads` — List all uploads for the current user
- `POST /api/uploads` — Upload a file (multipart/form-data)
- `GET /api/uploads/{id}` — Get upload metadata
- `DELETE /api/uploads/{id}` — Delete an upload

### Connectors (platform integrations)
- `GET /api/connectors` — List all connectors
- `POST /api/connectors` — Create a connector
- `GET /api/connectors/{id}` — Get connector by ID
- `PATCH /api/connectors/{id}` — Update a connector
- `DELETE /api/connectors/{id}` — Delete a connector

### Tools
- `GET /api/tools` — List all tools
- `POST /api/tools` — Create a tool
- `GET /api/tools/{id}` — Get tool by ID

## Chat Streaming (SSE)

`POST /api/conversations/{id}/chat/stream` returns a Server-Sent Events stream.

Each event contains a JSON payload:
- **Delta**: `{"delta": "text chunk"}` — incremental token from the LLM
- **Done**: `{"done": true, "id": "<message-uuid>"}` — stream finished; `id` is the saved message ID
- **Error**: `{"error": "message"}` — something went wrong

The frontend uses `fetch()` with `ReadableStream` (not `EventSource`) to support POST-based SSE.

## Error Handling

Errors are returned as JSON with appropriate HTTP status codes:

| Status | Meaning |
|---|---|
| 400 | Bad request — validation error or missing field |
| 401 | Unauthorized — no valid session cookie |
| 403 | Forbidden — authenticated but insufficient role/permissions |
| 404 | Not found |
| 409 | Conflict — e.g. email already registered |
| 500 | Internal server error |
